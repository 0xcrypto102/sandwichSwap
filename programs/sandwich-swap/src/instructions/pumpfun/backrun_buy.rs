use crate::error::ErrorCode;
use crate::instructions::pumpfun::bonding_curve::BondingCurveState;
use crate::instructions::pumpfun::{PumpFun, PUMPFUN_PROGRAM_ID};
use crate::sandwich_state::{SandwichCompleteEvent, SandwichState};
use anchor_lang::prelude::*;
use anchor_lang::prelude::{Account, Program, Signer, System};
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_program::account_info::AccountInfo;
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed;

#[derive(Accounts)]
#[instruction(sandwich_id: String)]
pub struct PumpFunBackrunBuyContext<'info> {
    /// CHECK: Global config
    pub global: AccountInfo<'info>,

    /// CHECK: Protocol fee recipient
    #[account(mut)]
    pub protocol_fee_recipient: AccountInfo<'info>,

    /// Base token mint (the token being bought or sold)
    pub mint: Box<Account<'info, Mint>>,

    /// CHECK: Bonding curve account
    #[account(mut)]
    pub bonding_curve: AccountLoader<'info, BondingCurveState>,

    /// Bonding curve token account
    #[account(mut)]
    pub bonding_curve_ata: Box<Account<'info, TokenAccount>>,

    /// User token account
    #[account(
        mut,
        close = user,
    )]
    pub user_ata: Box<Account<'info, TokenAccount>>,

    /// The user making the swap
    #[account(mut)]
    pub user: Signer<'info>,

    /// System program
    pub system_program: Program<'info, System>,

    /// CHECK: developer fee vault
    #[account(mut)]
    pub creator_fee_vault: AccountInfo<'info>,

    /// token program
    pub token_program: Program<'info, Token>,

    /// CHECK: Event authority account for PumpFun
    pub event_authority: AccountInfo<'info>,

    /// The pump amm program
    #[account(address = PUMPFUN_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub pump_program: Program<'info, PumpFun>,

    /// The account that stores sandwich state
    #[account(
       mut,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump = sandwich_state.bump,
       constraint = !sandwich_state.is_complete @ ErrorCode::SandwichAlreadyCompleted,
       constraint = sandwich_state.token_in_mint == *mint.to_account_info().key
           @ ErrorCode::TokenMintMismatch,
    )]
    pub sandwich_state: Account<'info, SandwichState>,
}

#[derive(AnchorSerialize)]
pub struct PumpFunSell {
    pub token_amount: u64,
    pub max_sol_cost: u64,
}

impl PumpFunSell {
    pub fn data(&self) -> Vec<u8> {
        let mut data = vec![149, 39, 222, 155, 211, 124, 152, 26]; // buy instruction discriminator
        data.extend_from_slice(&self.token_amount.to_le_bytes());
        data.extend_from_slice(&self.max_sol_cost.to_le_bytes());
        data
    }
}

pub fn pumpfun_backrun_buy(
    ctx: Context<PumpFunBackrunBuyContext>,
    sandwich_id: u64,
) -> Result<()> {
    let sandwich_state = &mut ctx.accounts.sandwich_state;

    let account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.global.key(), false),
        AccountMeta::new(ctx.accounts.protocol_fee_recipient.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint.key(), false),
        AccountMeta::new(ctx.accounts.bonding_curve.key(), false),
        AccountMeta::new(ctx.accounts.bonding_curve_ata.key(), false),
        AccountMeta::new(ctx.accounts.user_ata.key(), false),
        AccountMeta::new(ctx.accounts.user.key(), true),
        AccountMeta::new_readonly(ctx.accounts.system_program.key(), false),
        AccountMeta::new(ctx.accounts.creator_fee_vault.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new_readonly(ctx.accounts.event_authority.key(), false),
        AccountMeta::new_readonly(ctx.accounts.pump_program.key(), false)
    ];

    let accounts_vec = vec![
        ctx.accounts.global.to_account_info(),
        ctx.accounts.protocol_fee_recipient.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.bonding_curve.to_account_info(),
        ctx.accounts.bonding_curve_ata.to_account_info(),
        ctx.accounts.user_ata.to_account_info(),
        ctx.accounts.user.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.creator_fee_vault.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.event_authority.to_account_info(),
        ctx.accounts.pump_program.to_account_info(),
    ];

    let ix_data = PumpFunSell {
        token_amount: sandwich_state.frontrun_output_amount,
        max_sol_cost: 0,
    }.data();

    let sell_ix = Instruction {
        program_id: ctx.accounts.pump_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let output_token_balance_before = ctx.accounts.user.lamports();
    invoke_signed(&sell_ix, &accounts_vec, &[])?;

    sandwich_state.is_complete = true;

    // Calculate and store actual profit
    let output_token_balance_after = ctx.accounts.user.lamports();
    let actual_output = output_token_balance_after.saturating_sub(output_token_balance_before);
    let profit = actual_output.saturating_sub(sandwich_state.frontrun_input_amount);

    // Emit an event with profit information
    emit!(SandwichCompleteEvent {
        sandwich_id,
        profit,
        input_amount: sandwich_state.frontrun_input_amount,
        output_amount: actual_output,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}
