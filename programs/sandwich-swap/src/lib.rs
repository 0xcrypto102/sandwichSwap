// fixes unexpected `cfg` errors
// check https://solana.stackexchange.com/questions/17777/unexpected-cfg-condition-value-solana
#![allow(unexpected_cfgs)]

use anchor_lang::prelude::*;

declare_id!("XArSfgXtRWmxtyUW6dS6tTky1uYwpvaKEEq5eg93w15");

pub mod error;
pub mod instructions;
mod sandwich_state;

use instructions::*;

#[program]
pub mod sandwich_swap {
    use super::*;

    // Raydium AMM
    pub fn raydium_frontrun_amm_swap_base_in(
        ctx: Context<AmmFrontrunSwapBaseIn>,
        target_amount_in: u64,
        target_minimum_amount_out: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::amm_frontrun_swap_base_in(ctx, target_amount_in, target_minimum_amount_out, sandwich_id)
    }

    pub fn backrun_raydium_amm_swap_base_in(
        ctx: Context<AmmBackrunSwapBaseIn>,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::amm_backrun_swap_base_in(ctx, sandwich_id)
    }

    // Raydium CLMM
    pub fn raydium_clmm_swap<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ClmmSwap<'info>>,
        amount: u64,
        other_amount_threshold: u64,
        sqrt_price_limit_x64: u128,
        is_base_input: bool,
    ) -> Result<()> {
        instructions::clmm_swap(
            ctx,
            amount,
            other_amount_threshold,
            sqrt_price_limit_x64,
            is_base_input,
        )
    }

    pub fn raydium_clmm_frontrun_swap<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ClmmSandwichFrontrun<'info>>,
        target_amount: u64,
        target_other_amount_threshold: u64,
        target_sqrt_price_limit_x64: u128,
        target_is_base_input: bool,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::clmm_frontrun_swap(
            ctx,
            target_amount,
            target_other_amount_threshold,
            target_sqrt_price_limit_x64,
            target_is_base_input,
            sandwich_id,
        )
    }

    pub fn raydium_clmm_backrun_swap<'a, 'b, 'c: 'info, 'info>(
        ctx: Context<'a, 'b, 'c, 'info, ClmmSandwichBackrun<'info>>,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::clmm_backrun_swap(ctx, sandwich_id)
    }


    // Raydium CPMM
    pub fn raydium_cpmm_swap_base_input(
        ctx: Context<CpmmSwapBaseInput>,
        amount_in: u64,
        minimum_amount_out: u64,
    ) -> Result<()> {
        instructions::cpmm_swap_base_input(ctx, amount_in, minimum_amount_out)
    }

    pub fn raydium_cpmm_swap_base_output(
        ctx: Context<CpmmSwapBaseOutput>,
        max_amount_in: u64,
        amount_out: u64,
    ) -> Result<()> {
        instructions::cpmm_swap_base_output(ctx, max_amount_in, amount_out)
    }

    pub fn raydium_cpmm_frontrun_swap_base_output(
        ctx: Context<CpmmSandwichFrontrunOutput>,
        target_max_amount_in: u64,
        target_amount_out: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::cpmm_frontrun_swap_base_output(
            ctx,
            target_max_amount_in,
            target_amount_out,
            sandwich_id,
        )
    }

    pub fn raydium_cpmm_backrun_swap_base_output(
        ctx: Context<CpmmSandwichBackrunOutput>,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::cpmm_backrun_swap_base_output(ctx, sandwich_id)
    }

    pub fn raydium_cpmm_frontrun_swap_base_input(
        ctx: Context<CpmmSandwichFrontrun>,
        target_amount_in: u64,
        target_minimum_amount_out: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::cpmm_frontrun_swap_base_input(
            ctx,
            target_amount_in,
            target_minimum_amount_out,
            sandwich_id,
        )
    }

    pub fn raydium_cpmm_backrun_swap_base_input(
        ctx: Context<CpmmSandwichBackrun>,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::cpmm_backrun_swap_base_input(ctx, sandwich_id)
    }
    
    pub fn pump_frontrun_buy(
        ctx: Context<PumpSwapContext>,
        base_amount_out: u64,
        max_quote_amount_in: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::pumpswap_frontrun_buy(ctx, base_amount_out, max_quote_amount_in, sandwich_id)
    }
    
    pub fn pump_frontrun_sell(
        ctx: Context<PumpSwapContext>,
        base_amount_in: u64,
        min_quote_amount_out: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::pumpswap_frontrun_sell(ctx, base_amount_in, min_quote_amount_out, sandwich_id)
    }
    
    pub fn pump_backrun_buy(
        ctx: Context<PumpSwapContext>,
    ) -> Result<()> {
        instructions::pumpswap_backrun_buy(ctx)
    }
    
    pub fn pump_backrun_sell(
        ctx: Context<PumpSwapContext>,
    ) -> Result<()> {
        instructions::pumpswap_backrun_sell(ctx)
    }

    // PumpFun
    pub fn pumpfun_frontrun_buy(
        ctx: Context<PumpFunFrontrunBuyContext>,
        target_base_amount_out: u64,
        target_max_quote_amount_in: u64,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::pumpfun_frontrun_buy(ctx, target_base_amount_out, target_max_quote_amount_in, sandwich_id)
    }

    pub fn pumpfun_backrun_buy(
        ctx: Context<PumpFunBackrunBuyContext>,
        sandwich_id: u64,
    ) -> Result<()> {
        instructions::pumpfun_backrun_buy(ctx, sandwich_id)
    }

}
