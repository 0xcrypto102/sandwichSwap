use anchor_lang::AnchorSerialize;
use solana_program::pubkey::Pubkey;
use anchor_lang::prelude::*;

pub mod frontrun_buy;
pub use frontrun_buy::*;

pub mod backrun_buy;
mod bonding_curve;

pub use backrun_buy::*;

// PumpFun program ID
pub const PUMPFUN_PROGRAM_ID: &str = "6EF8rrecthR5Dkzon8Nwu78hRvfCKubJ14M5uBEwF6P";

#[derive(Clone)]
pub struct PumpFun;

impl anchor_lang::Id for PumpFun {
    fn id() -> Pubkey {
        PUMPFUN_PROGRAM_ID.parse::<Pubkey>().unwrap()
    }
}

#[derive(AnchorSerialize)]
pub struct Swap {
    pub discriminator: u8,
    pub amount_in: u64,
    pub min_amount_out: u64,
}

impl Swap {
    pub fn data(&self) -> Vec<u8> {
        let mut data = vec![250, 234, 13, 123, 213, 156, 19, 236]; // buy instruction discriminator
        data.extend_from_slice(&self.discriminator.to_le_bytes());
        data.extend_from_slice(&self.amount_in.to_le_bytes());
        data.extend_from_slice(&self.min_amount_out.to_le_bytes());
        data
    }
}
