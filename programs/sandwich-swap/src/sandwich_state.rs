use anchor_lang::{account, event};
use solana_program::pubkey::Pubkey;
use anchor_lang::prelude::*;

#[account]
pub struct SandwichState {
    pub frontrun_output_amount: u64, // Amount of tokens obtained from frontrun
    pub frontrun_input_amount: u64,  // Amount of tokens spent in frontrun
    pub target_tx_signature: [u8; 64], // Target tx signature for tracking
    pub sandwich_id: u64,            // Unique identifier for this sandwich
    pub is_complete: bool,           // Flag to prevent double execution
    pub token_in_mint: Pubkey,       // Input token mint (for verification)
    pub token_out_mint: Pubkey,      // Output token mint (for verification)
    pub timestamp: i64,              // Timestamp for tracking
    pub bump: u8,                    // PDA bump
}

impl SandwichState {
    pub const SIZE: usize = 8 + 8 + 64 + 8 + 1 + 32 + 32 + 8 + 1; // Size in bytes
}

#[event]
pub struct SandwichCompleteEvent {
    pub sandwich_id: u64,
    pub profit: u64,
    pub input_amount: u64,
    pub output_amount: u64,
    pub timestamp: i64,
}
