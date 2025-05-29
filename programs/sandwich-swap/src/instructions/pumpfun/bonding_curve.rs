use anchor_lang::prelude::*;
use solana_program::pubkey::Pubkey;

#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct BondingCurveState {
    // virtual made token reserves
    pub virtual_token_reserves: u64,

    // virtual made sol reserves (already starts a t ~30 SOL)
    pub virtual_sol_reserves: u64,

    // real token reserves
    pub real_token_reserves: u64,

    // real sol reserves
    pub real_sol_reserves: u64,

    // token supply in total
    pub token_total_supply: u64,

    // if already moved to pumpswap
    pub complete: bool,

    // wallet that created that token
    pub creator: Pubkey,
}