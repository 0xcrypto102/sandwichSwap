use crate::error::ErrorCode;
use crate::instructions::{AmmAuthority, AMM_AUTHORITY_ID, Serum, SERUM_PROGRAM_ID, Amm, AMM_PROGRAM_ID, Swap};
use crate::sandwich_state::{SandwichCompleteEvent, SandwichState};
use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed;
use crate::instructions::amm::pair::ProgramAccount;

#[derive(Accounts, Clone)]
#[instruction(sandwich_id: String)]
pub struct AmmBackrunSwapBaseIn<'info> {
    /// token program
    pub token_program: Program<'info, Token>,

    /// CHECK Pair account
    #[account(mut)]
    pub amm: AccountLoader<'info, ProgramAccount>,

    /// Raydium Authority
    #[account(address = AMM_AUTHORITY_ID.parse::<Pubkey>().unwrap())]
    pub amm_authority: Program<'info, AmmAuthority>,

    /// CHECK Open Orders account
    #[account(mut)]
    pub amm_open_orders: AccountInfo<'info>,

    /// CHECK Target Orders account
    #[account(mut)]
    pub amm_target_orders: AccountInfo<'info>,

    /// Pool base token account
    #[account(mut)]
    pub pool_coin_token_account: Box<Account<'info, TokenAccount>>,

    /// Pool quote token account
    #[account(mut)]
    pub pool_pc_token_account: Box<Account<'info, TokenAccount>>,

    /// OpenBook program id
    #[account(address = SERUM_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub serum_program: Program<'info, Serum>,

    /// CHECK Serum market account
    #[account(mut)]
    pub serum_market: AccountInfo<'info>,

    /// CHECK Serum bids account
    #[account(mut)]
    pub serum_bids: AccountInfo<'info>,

    /// CHECK Serum asks account
    #[account(mut)]
    pub serum_asks: AccountInfo<'info>,

    /// CHECK Serum event queue account
    #[account(mut)]
    pub serum_event_queue: AccountInfo<'info>,

    /// Pool base token account
    #[account(mut)]
    pub serum_coin_vault_account: Box<Account<'info, TokenAccount>>,

    /// Pool quote token account
    #[account(mut)]
    pub serum_pc_vault_account: Box<Account<'info, TokenAccount>>,

    /// CHECK Serum vault signer account
    pub serum_vault_signer: AccountInfo<'info>,

    /// User source token account
    #[account(mut)]
    pub user_source_token_account: Box<Account<'info, TokenAccount>>,

    /// User destination token account
    #[account(
        mut,
        close = user_source_owner,
    )]
    pub user_target_token_account: Box<Account<'info, TokenAccount>>,

    /// The user making the swap
    #[account(mut)]
    pub user_source_owner: Signer<'info>,

    /// The account that stores sandwich state
    #[account(
       mut,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump = sandwich_state.bump,
       constraint = !sandwich_state.is_complete @ ErrorCode::SandwichAlreadyCompleted,
    )]
    pub sandwich_state: Account<'info, SandwichState>,

    /// AMM Program
    #[account(address = AMM_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub amm_program: Program<'info, Amm>,

    /// base mint
    #[account(
        constraint = base_mint.key() == amm.load()?.base_mint
    )]
    pub base_mint: Account<'info, Mint>,
}

/// swap_base_in instruction
pub fn amm_backrun_swap_base_in(
    ctx: Context<AmmBackrunSwapBaseIn>,
    sandwich_id: u64,
) -> Result<()> {
    let sandwich_state = &mut ctx.accounts.sandwich_state;

    let account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new(ctx.accounts.amm.key(), false),
        AccountMeta::new_readonly(ctx.accounts.amm_authority.key(), false),
        AccountMeta::new(ctx.accounts.amm_open_orders.key(), false),
        AccountMeta::new(ctx.accounts.amm_target_orders.key(), false),
        AccountMeta::new(ctx.accounts.pool_coin_token_account.key(), false),
        AccountMeta::new(ctx.accounts.pool_pc_token_account.key(), false),
        AccountMeta::new_readonly(ctx.accounts.serum_program.key(), false),
        AccountMeta::new(ctx.accounts.serum_market.key(), false),
        AccountMeta::new(ctx.accounts.serum_bids.key(), false),
        AccountMeta::new(ctx.accounts.serum_asks.key(), false),
        AccountMeta::new(ctx.accounts.serum_event_queue.key(), false),
        AccountMeta::new(ctx.accounts.serum_coin_vault_account.key(), false),
        AccountMeta::new(ctx.accounts.serum_pc_vault_account.key(), false),
        AccountMeta::new_readonly(ctx.accounts.serum_vault_signer.key(), false),
        AccountMeta::new(ctx.accounts.user_source_token_account.key(), false),
        AccountMeta::new(ctx.accounts.user_target_token_account.key(), false),
        AccountMeta::new(ctx.accounts.user_source_owner.key(), true),
    ];

    let accounts_vec = vec![
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.amm.to_account_info(),
        ctx.accounts.amm_authority.to_account_info(),
        ctx.accounts.amm_open_orders.to_account_info(),
        ctx.accounts.amm_target_orders.to_account_info(),
        ctx.accounts.pool_coin_token_account.to_account_info(),
        ctx.accounts.pool_pc_token_account.to_account_info(),
        ctx.accounts.serum_program.to_account_info(),
        ctx.accounts.serum_market.to_account_info(),
        ctx.accounts.serum_bids.to_account_info(),
        ctx.accounts.serum_asks.to_account_info(),
        ctx.accounts.serum_event_queue.to_account_info(),
        ctx.accounts.serum_coin_vault_account.to_account_info(),
        ctx.accounts.serum_pc_vault_account.to_account_info(),
        ctx.accounts.serum_vault_signer.to_account_info(),
        ctx.accounts.user_source_token_account.to_account_info(),
        ctx.accounts.user_target_token_account.to_account_info(),
        ctx.accounts.user_source_owner.to_account_info(),
    ];

    let ix_data = Swap {
        discriminator: 9,
        amount_in: sandwich_state.frontrun_output_amount,
        min_amount_out: 0,
    }.data();

    let buy_ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let output_token_balance_before = ctx.accounts.user_target_token_account.amount;
    invoke_signed(&buy_ix, &accounts_vec, &[])?;

    sandwich_state.is_complete = true;

    ctx.accounts.user_source_token_account.reload()?;
    let output_token_balance_after = ctx.accounts.user_source_token_account.amount;
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
