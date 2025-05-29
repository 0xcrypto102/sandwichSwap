pub mod frontrun_swap_base_in;

use solana_program::pubkey::Pubkey;
pub use frontrun_swap_base_in::*;

pub mod backrun_swap_base_in;
mod pair;

pub use backrun_swap_base_in::*;

// AMM program ID
pub const AMM_PROGRAM_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

#[derive(Clone)]
pub struct Amm;

impl anchor_lang::Id for Amm {
    fn id() -> Pubkey {
        AMM_PROGRAM_ID.parse::<Pubkey>().unwrap()
    }
}

// Serum program ID
pub const SERUM_PROGRAM_ID: &str = "srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX";

#[derive(Clone)]
pub struct Serum;

impl anchor_lang::Id for Serum {
    fn id() -> Pubkey {
        SERUM_PROGRAM_ID.parse::<Pubkey>().unwrap()
    }
}

// AMM program ID
pub const AMM_AUTHORITY_ID: &str = "675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8";

#[derive(Clone)]
pub struct AmmAuthority;

impl anchor_lang::Id for AmmAuthority {
    fn id() -> Pubkey {
        AMM_AUTHORITY_ID.parse::<Pubkey>().unwrap()
    }
}
