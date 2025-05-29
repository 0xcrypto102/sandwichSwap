use anchor_lang::prelude::*;
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};

use crate::{instructions::{PumpSwapGlobalConfig, PumpSwapPoolState}, sandwich_state::SandwichState};

// PumpSwap program ID
pub const PUMP_AMM_PROGRAM_ID: &str = "pAMMBay6oceH9fJKBRHGP5D4bD4sWpmSwMn52FMfXEA";

#[derive(Accounts)]
#[instruction(sandwich_id: u64)]
pub struct PumpSwapContext<'info> {
    /// The pump amm program
    #[account(address = PUMP_AMM_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub pump_amm_program: Program<'info, PumpAmm>,

    /// CHECK: This is the pool account from PumpSwap, verified by CPI
    #[account(mut)]
    pub pool: AccountLoader<'info, PumpSwapPoolState>,

    /// The user making the swap
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: This is the global config account from PumpSwap, verified by CPI
    pub global_config: AccountLoader<'info, PumpSwapGlobalConfig>,

    /// Base token mint (the token being bought or sold)
    pub base_mint: Box<Account<'info, Mint>>,

    /// Quote token mint (typically a stablecoin or major token)
    pub quote_mint: Box<Account<'info, Mint>>,

    /// User's base token account
    #[account(mut)]
    pub user_base_token_account: Box<Account<'info, TokenAccount>>,

    /// User's quote token account
    #[account(mut)]
    pub user_quote_token_account: Box<Account<'info, TokenAccount>>,

    /// Pool's base token account
    #[account(mut)]
    pub pool_base_token_account: Box<Account<'info, TokenAccount>>,

    /// Pool's quote token account
    #[account(mut)]
    pub pool_quote_token_account: Box<Account<'info, TokenAccount>>,

    /// CHECK: Protocol fee recipient, verified by PumpSwap during CPI
    pub protocol_fee_recipient: AccountInfo<'info>,

    /// CHECK: Protocol fee recipient token account, verified by PumpSwap during CPI
    #[account(mut)]
    pub protocol_fee_recipient_token_account: AccountInfo<'info>,

    /// Token program for the base token
    pub base_token_program: Program<'info, Token>,

    /// Token program for the quote token
    pub quote_token_program: Program<'info, Token>,

    /// System program
    pub system_program: Program<'info, System>,

    /// Associated token program
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// CHECK: Event authority account for PumpSwap, verified by CPI
    pub event_authority: AccountInfo<'info>,

    /// CHECK: PumpSwap program account for the CPI
    pub program: AccountInfo<'info>,

    /// CHECK: Coin creator vault ATA, optional account for creator fees
    #[account(mut)]
    pub coin_creator_vault_ata: Option<AccountInfo<'info>>,

    /// CHECK: Coin creator vault authority, optional PDA for creator fees
    pub coin_creator_vault_authority: Option<AccountInfo<'info>>,
    
    /// The account that will store sandwich state
    #[account(
       init_if_needed,
       payer = user,
       space = 8 + SandwichState::SIZE,
       seeds = [b"sandwich", &sandwich_id.to_le_bytes()],
       bump
   )]
    pub sandwich_state: Account<'info, SandwichState>,
}

#[derive(Clone)]
pub struct PumpAmm;

impl anchor_lang::Id for PumpAmm {
    fn id() -> Pubkey {
        PUMP_AMM_PROGRAM_ID.parse::<Pubkey>().unwrap()
    }
}
