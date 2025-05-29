use anchor_lang::prelude::*;
use anchor_spl::{
    memo::Memo,
    token::Token,
    token_2022::spl_token_2022::{
        self,
        extension::{
            transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
            BaseStateWithExtensions, StateWithExtensions,
        },
    },
    token_interface::{Mint, Token2022, TokenAccount},
};
use raydium_clmm_cpi::{cpi, program::RaydiumClmm};

use crate::{
    error::ErrorCode,
    sandwich_state::{SandwichCompleteEvent, SandwichState},
};

// Number of ObservationState element
pub const OBSERVATION_NUM: usize = 1000;
// Number of rewards Token
pub const REWARD_NUM: usize = 3;

pub const Q64: u128 = (u64::MAX as u128) + 1; // 2^64
                                              ///// The minimum value that can be returned from #get_sqrt_price_at_tick. Equivalent to get_sqrt_price_at_tick(MIN_TICK)
pub const MIN_SQRT_PRICE_X64: u128 = 4295048016;
/// The maximum value that can be returned from #get_sqrt_price_at_tick. Equivalent to get_sqrt_price_at_tick(MAX_TICK)
pub const MAX_SQRT_PRICE_X64: u128 = 79226673521066979257578248091;

// We define this here instead of importing AmmConfig to avoid duplicate
// accounts error during idl building
// details here https://github.com/solana-foundation/anchor/issues/3500
#[account]
#[derive(Default, Debug)]
pub struct ClmmAmmConfig {
    /// Bump to identify PDA
    pub bump: u8,
    pub index: u16,
    /// Address of the protocol owner
    pub owner: Pubkey,
    /// The protocol fee
    pub protocol_fee_rate: u32,
    /// The trade fee, denominated in hundredths of a bip (10^-6)
    pub trade_fee_rate: u32,
    /// The tick spacing
    pub tick_spacing: u16,
    /// The fund fee, denominated in hundredths of a bip (10^-6)
    pub fund_fee_rate: u32,
    // padding space for upgrade
    pub padding_u32: u32,
    pub fund_owner: Pubkey,
    pub padding: [u64; 3],
}

/// The element of observations in ObservationState
#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ClmmObservation {
    /// The block timestamp of the observation
    pub block_timestamp: u32,
    /// the price of the observation timestamp, Q64.64
    pub sqrt_price_x64: u128,
    /// the cumulative of price during the duration time, Q64.64
    pub cumulative_time_price_x64: u128,
    /// padding for feature update
    pub padding: u128,
}

// We define this here instead of importing ObservationState to avoid duplicate
// accounts error during idl building
// details here https://github.com/solana-foundation/anchor/issues/3500
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
pub struct ClmmObservationState {
    /// Whether the ObservationState is initialized
    pub initialized: bool,
    pub pool_id: Pubkey,
    /// observation array
    pub observations: [ClmmObservation; OBSERVATION_NUM],
    /// padding for feature update
    pub padding: [u128; 5],
}

// We define this here instead of importing PoolState to avoid duplicate
// accounts error during idl building
// details here https://github.com/solana-foundation/anchor/issues/3500
/// The pool state
///
/// PDA of `[POOL_SEED, config, token_mint_0, token_mint_1]`
///
#[account(zero_copy(unsafe))]
#[repr(C, packed)]
#[derive(Default, Debug)]
pub struct ClmmPoolState {
    /// Bump to identify PDA
    pub bump: [u8; 1],
    // Which config the pool belongs
    pub amm_config: Pubkey,
    // Pool creator
    pub owner: Pubkey,

    /// Token pair of the pool, where token_mint_0 address < token_mint_1 address
    pub token_mint_0: Pubkey,
    pub token_mint_1: Pubkey,

    /// Token pair vault
    pub token_vault_0: Pubkey,
    pub token_vault_1: Pubkey,

    /// observation account key
    pub observation_key: Pubkey,

    /// mint0 and mint1 decimals
    pub mint_decimals_0: u8,
    pub mint_decimals_1: u8,

    /// The minimum number of ticks between initialized ticks
    pub tick_spacing: u16,
    /// The currently in range liquidity available to the pool.
    pub liquidity: u128,
    /// The current price of the pool as a sqrt(token_1/token_0) Q64.64 value
    pub sqrt_price_x64: u128,
    /// The current tick of the pool, i.e. according to the last tick transition that was run.
    pub tick_current: i32,

    /// the most-recently updated index of the observations array
    pub observation_index: u16,
    pub observation_update_duration: u16,

    /// The fee growth as a Q64.64 number, i.e. fees of token_0 and token_1 collected per
    /// unit of liquidity for the entire life of the pool.
    pub fee_growth_global_0_x64: u128,
    pub fee_growth_global_1_x64: u128,

    /// The amounts of token_0 and token_1 that are owed to the protocol.
    pub protocol_fees_token_0: u64,
    pub protocol_fees_token_1: u64,

    /// The amounts in and out of swap token_0 and token_1
    pub swap_in_amount_token_0: u128,
    pub swap_out_amount_token_1: u128,
    pub swap_in_amount_token_1: u128,
    pub swap_out_amount_token_0: u128,

    /// Bitwise representation of the state of the pool
    /// bit0, 1: disable open position and increase liquidity, 0: normal
    /// bit1, 1: disable decrease liquidity, 0: normal
    /// bit2, 1: disable collect fee, 0: normal
    /// bit3, 1: disable collect reward, 0: normal
    /// bit4, 1: disable swap, 0: normal
    pub status: u8,
    /// Leave blank for future use
    pub padding: [u8; 7],

    pub reward_infos: [RewardInfo; REWARD_NUM],

    /// Packed initialized tick array state
    pub tick_array_bitmap: [u64; 16],

    /// except protocol_fee and fund_fee
    pub total_fees_token_0: u64,
    /// except protocol_fee and fund_fee
    pub total_fees_claimed_token_0: u64,
    pub total_fees_token_1: u64,
    pub total_fees_claimed_token_1: u64,

    pub fund_fees_token_0: u64,
    pub fund_fees_token_1: u64,

    // The timestamp allowed for swap in the pool.
    pub open_time: u64,

    // Unused bytes for future upgrades.
    pub padding1: [u64; 25],
    pub padding2: [u64; 32],
}

#[zero_copy(unsafe)]
#[repr(C, packed)]
#[derive(Default, Debug, PartialEq, Eq)]
pub struct RewardInfo {
    /// Reward state
    pub reward_state: u8,
    /// Reward open time
    pub open_time: u64,
    /// Reward end time
    pub end_time: u64,
    /// Reward last update time
    pub last_update_time: u64,
    /// Q64.64 number indicates how many tokens per second are earned per unit of liquidity.
    pub emissions_per_second_x64: u128,
    /// The total amount of reward emissioned
    pub reward_total_emissioned: u64,
    /// The total amount of claimed reward
    pub reward_claimed: u64,
    /// Reward token mint.
    pub token_mint: Pubkey,
    /// Reward vault token account.
    pub token_vault: Pubkey,
    /// The owner that has permission to set reward param
    pub authority: Pubkey,
    /// Q64.64 number that tracks the total tokens earned per unit of liquidity since the reward
    /// emissions were turned on.
    pub reward_growth_global_x64: u128,
}

/// Memo msg for swap
pub const SWAP_MEMO_MSG: &[u8] = b"raydium_swap";
#[derive(Accounts)]
pub struct ClmmSwap<'info> {
    pub clmm_program: Program<'info, RaydiumClmm>,
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, ClmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, ClmmPoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(mut)]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(mut)]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ClmmObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// SPL program 2022 for token transfers
    pub token_program_2022: Program<'info, Token2022>,

    /// memo program
    pub memo_program: Program<'info, Memo>,

    /// The mint of token vault 0
    #[account(
        address = input_vault.mint
    )]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of token vault 1
    #[account(
        address = output_vault.mint
    )]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,
    // remaining accounts
    // tickarray_bitmap_extension: must add account if need regardless the sequence
    // tick_array_account_1
    // tick_array_account_2
    // tick_array_account_...
}

pub fn clmm_swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ClmmSwap<'info>>,
    amount: u64,
    other_amount_threshold: u64,
    sqrt_price_limit_x64: u128,
    is_base_input: bool,
) -> Result<()> {
    let cpi_accounts = cpi::accounts::SwapSingleV2 {
        payer: ctx.accounts.payer.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
        token_program: ctx.accounts.token_program.to_account_info(),
        token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        input_vault_mint: ctx.accounts.input_vault_mint.to_account_info(),
        output_vault_mint: ctx.accounts.output_vault_mint.to_account_info(),
    };
    let cpi_context = CpiContext::new(ctx.accounts.clmm_program.to_account_info(), cpi_accounts)
        .with_remaining_accounts(ctx.remaining_accounts.to_vec());
    cpi::swap_v2(
        cpi_context,
        amount,
        other_amount_threshold,
        sqrt_price_limit_x64,
        is_base_input,
    )
}

#[derive(Accounts)]
#[instruction(sandwich_id: String)]
pub struct ClmmSandwichFrontrun<'info> {
    pub clmm_program: Program<'info, RaydiumClmm>,

    /// The user performing the swap
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, ClmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, ClmmPoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(
      mut,
      constraint = input_vault.key() == pool_state.load()?.token_vault_0 || input_vault.key() == pool_state.load()?.token_vault_1
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(
      mut,
      constraint = output_vault.key() == pool_state.load()?.token_vault_0 || output_vault.key() == pool_state.load()?.token_vault_1
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ClmmObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// SPL program 2022 for token transfers
    pub token_program_2022: Program<'info, Token2022>,

    /// memo program
    pub memo_program: Program<'info, Memo>,

    /// The mint of input token
    #[account(address = input_vault.mint)]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token
    #[account(address = output_vault.mint)]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The account that will store sandwich state
    #[account(
       init,
       payer = payer,
       space = 8 + SandwichState::SIZE,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump
    )]
    pub sandwich_state: Account<'info, SandwichState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(sandwich_id: String)]
pub struct ClmmSandwichBackrun<'info> {
    pub clmm_program: Program<'info, RaydiumClmm>,

    /// The user performing the swap
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, ClmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, ClmmPoolState>,

    /// The user token account for input token (was output in frontrun)
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token (was input in frontrun)
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token (was output in frontrun)
    #[account(
      mut,
      constraint = input_vault.key() == pool_state.load()?.token_vault_0 || input_vault.key() == pool_state.load()?.token_vault_1
    )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token (was input in frontrun)
    #[account(
      mut,
      constraint = output_vault.key() == pool_state.load()?.token_vault_0 || output_vault.key() == pool_state.load()?.token_vault_1
    )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, ClmmObservationState>,

    /// SPL program for token transfers
    pub token_program: Program<'info, Token>,

    /// SPL program 2022 for token transfers
    pub token_program_2022: Program<'info, Token2022>,

    /// memo program
    pub memo_program: Program<'info, Memo>,

    /// The mint of input token (was output in frontrun)
    #[account(address = input_vault.mint)]
    pub input_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token (was input in frontrun)
    #[account(address = output_vault.mint)]
    pub output_vault_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The account that stores sandwich state
    #[account(
       mut,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump = sandwich_state.bump,
       constraint = !sandwich_state.is_complete @ ErrorCode::SandwichAlreadyCompleted,
       constraint = sandwich_state.token_in_mint == *output_vault_mint.to_account_info().key
           @ ErrorCode::TokenMintMismatch,
       constraint = sandwich_state.token_out_mint == *input_vault_mint.to_account_info().key
           @ ErrorCode::TokenMintMismatch
    )]
    pub sandwich_state: Account<'info, SandwichState>,
}

pub fn clmm_frontrun_swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ClmmSandwichFrontrun<'info>>,
    target_amount: u64,
    target_other_amount_threshold: u64,
    target_sqrt_price_limit_x64: u128,
    target_is_base_input: bool,
    sandwich_id: u64,
) -> Result<()> {
    // Load pool state to get current price and liquidity
    let pool_state = ctx.accounts.pool_state.load()?;
    let current_sqrt_price_x64 = pool_state.sqrt_price_x64;
    let current_tick = pool_state.tick_current;
    let liquidity = pool_state.liquidity;

    // Check if the pool is open for trading
    require_gt!(Clock::get()?.unix_timestamp as u64, pool_state.open_time);

    // Determine the swap direction
    let zero_for_one = ctx.accounts.input_vault.mint == pool_state.token_mint_0;

    // Calculate adjustments for transfer fees if needed
    let target_actual_amount = if target_is_base_input {
        let transfer_fee =
            clmm_get_transfer_fee(*ctx.accounts.input_vault_mint.clone(), target_amount)?;
        target_amount.saturating_sub(transfer_fee)
    } else {
        let transfer_fee =
            clmm_get_transfer_inverse_fee(*ctx.accounts.output_vault_mint.clone(), target_amount)?;
        target_amount.saturating_add(transfer_fee)
    };

    // Calculate target's slippage tolerance in basis points
    let target_slippage_bps = calculate_clmm_slippage(
        target_actual_amount,
        target_other_amount_threshold,
        target_is_base_input,
        target_sqrt_price_limit_x64,
        current_sqrt_price_x64,
        current_tick,
        liquidity,
        zero_for_one,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Use 95% of target's slippage tolerance to ensure their tx succeeds
    let safe_slippage_bps = target_slippage_bps.saturating_mul(95).saturating_div(100);

    // Calculate optimal sandwich amount through binary search
    let optimal_amount = calculate_optimal_clmm_sandwich_amount(
        current_sqrt_price_x64,
        current_tick,
        liquidity,
        target_actual_amount,
        safe_slippage_bps,
        target_is_base_input,
        zero_for_one,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Ensure calculated amount is reasonable
    if optimal_amount < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    // Calculate appropriate sqrt_price_limit_x64 for our frontrun transaction
    let frontrun_sqrt_price_limit_x64 = if zero_for_one {
        // Limit how far down the price can go to ensure target transaction success
        let price_impact = calculate_price_impact(
            current_sqrt_price_x64,
            liquidity,
            optimal_amount,
            zero_for_one,
            true, // Always exact input for frontrun
            ctx.accounts.amm_config.trade_fee_rate,
        )?;

        let min_allowed_price = if target_sqrt_price_limit_x64 > 0 {
            // If target specified a price limit, respect it
            std::cmp::max(
                target_sqrt_price_limit_x64,
                current_sqrt_price_x64.saturating_sub(price_impact),
            )
        } else {
            current_sqrt_price_x64.saturating_sub(price_impact)
        };

        // Ensure we don't go below the absolute minimum allowed
        std::cmp::max(MIN_SQRT_PRICE_X64 + 1, min_allowed_price)
    } else {
        // Limit how high the price can go to ensure target transaction success
        let price_impact = calculate_price_impact(
            current_sqrt_price_x64,
            liquidity,
            optimal_amount,
            zero_for_one,
            true, // Always exact input for frontrun
            ctx.accounts.amm_config.trade_fee_rate,
        )?;

        let max_allowed_price = if target_sqrt_price_limit_x64 > 0 {
            // If target specified a price limit, respect it
            std::cmp::min(
                target_sqrt_price_limit_x64,
                current_sqrt_price_x64.saturating_add(price_impact),
            )
        } else {
            current_sqrt_price_x64.saturating_add(price_impact)
        };

        // Ensure we don't go above the absolute maximum allowed
        std::cmp::min(MAX_SQRT_PRICE_X64 - 1, max_allowed_price)
    };

    // Record initial balances
    let output_token_balance_before = ctx.accounts.output_token_account.amount;
    let input_token_balance_before = ctx.accounts.input_token_account.amount;

    // Execute frontrun swap
    let cpi_accounts = cpi::accounts::SwapSingleV2 {
        payer: ctx.accounts.payer.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
        token_program: ctx.accounts.token_program.to_account_info(),
        token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        input_vault_mint: ctx.accounts.input_vault_mint.to_account_info(),
        output_vault_mint: ctx.accounts.output_vault_mint.to_account_info(),
    };

    let cpi_context = CpiContext::new(ctx.accounts.clmm_program.to_account_info(), cpi_accounts)
        .with_remaining_accounts(ctx.remaining_accounts.to_vec());

    // For frontrun we want exact input to ensure proper price impact
    cpi::swap_v2(
        cpi_context,
        optimal_amount, // Exact amount calculated for maximum profit within slippage limits
        0,              // No minimum output requirement - we accept whatever the market gives us
        frontrun_sqrt_price_limit_x64,
        true, // Always base input for frontrun for predictable price impact
    )?;

    // Reload token accounts to get actual amounts
    ctx.accounts.output_token_account.reload()?;
    ctx.accounts.input_token_account.reload()?;

    // Calculate actual amounts used in frontrun
    let frontrun_output_amount = ctx
        .accounts
        .output_token_account
        .amount
        .checked_sub(output_token_balance_before)
        .unwrap();

    let frontrun_input_amount = input_token_balance_before
        .checked_sub(ctx.accounts.input_token_account.amount)
        .unwrap();

    // Store frontrun data in PDA for backrun
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = frontrun_output_amount;
    sandwich_state.frontrun_input_amount = frontrun_input_amount;
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_in_mint = *ctx.accounts.input_vault_mint.to_account_info().key;
    sandwich_state.token_out_mint = *ctx.accounts.output_vault_mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;

    Ok(())
}

pub fn clmm_backrun_swap<'a, 'b, 'c: 'info, 'info>(
    ctx: Context<'a, 'b, 'c, 'info, ClmmSandwichBackrun<'info>>,
    sandwich_id: u64,
) -> Result<()> {
    // Get the exact amounts from the frontrun transaction
    let frontrun_output = ctx.accounts.sandwich_state.frontrun_output_amount;
    let frontrun_input = ctx.accounts.sandwich_state.frontrun_input_amount;

    // Load pool state to get current price (after target tx)
    let pool_state = ctx.accounts.pool_state.load()?;
    let current_sqrt_price_x64 = pool_state.sqrt_price_x64;
    let current_tick = pool_state.tick_current;
    let liquidity = pool_state.liquidity;

    // Check if the pool is open for trading
    require_gt!(Clock::get()?.unix_timestamp as u64, pool_state.open_time);

    // Determine trade direction for backrun (opposite of frontrun direction)
    let zero_for_one = ctx.accounts.input_vault.mint == pool_state.token_mint_0;

    // Calculate transfer fee adjustment if needed
    let amount_with_fee = if *ctx.accounts.input_vault_mint.to_account_info().owner == Token::id() {
        // No transfer fees for regular SPL tokens
        frontrun_output
    } else {
        // For token-2022 tokens with transfer fees
        let transfer_fee =
            clmm_get_transfer_fee(*ctx.accounts.input_vault_mint.clone(), frontrun_output)?;
        frontrun_output.saturating_sub(transfer_fee)
    };

    // Calculate expected output from backrun based on current market conditions
    let raw_expected_output = simulate_clmm_swap_output(
        current_sqrt_price_x64,
        current_tick,
        liquidity,
        amount_with_fee,
        zero_for_one,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Apply any transfer fees on output token if applicable
    let expected_output = if *ctx.accounts.output_vault_mint.to_account_info().owner == Token::id()
    {
        // No transfer fees for regular SPL tokens
        raw_expected_output
    } else {
        // For token-2022 tokens with transfer fees
        let inverse_fee = clmm_get_transfer_inverse_fee(
            *ctx.accounts.output_vault_mint.clone(),
            raw_expected_output,
        )?;
        raw_expected_output.saturating_sub(inverse_fee)
    };

    // Calculate minimum acceptable output for backrun for profitability
    let min_profit_factor = 1005; // 0.5% minimum profit
    let min_required_output = frontrun_input
        .checked_mul(min_profit_factor)
        .ok_or(ErrorCode::CalculationFailure)?
        .checked_div(1000)
        .ok_or(ErrorCode::CalculationFailure)?;

    // Use max of expected output with safety margin or minimum required output
    let minimum_output = std::cmp::max(
        expected_output.saturating_mul(98).saturating_div(100), // 2% safety margin
        min_required_output,
    );

    // Verify potential profitability
    if minimum_output <= frontrun_input {
        return err!(ErrorCode::UnprofitableSandwich);
    }

    // Record initial balances
    let output_token_balance_before = ctx.accounts.output_token_account.amount;

    // Execute the backrun swap
    let cpi_accounts = cpi::accounts::SwapSingleV2 {
        payer: ctx.accounts.payer.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
        token_program: ctx.accounts.token_program.to_account_info(),
        token_program_2022: ctx.accounts.token_program_2022.to_account_info(),
        memo_program: ctx.accounts.memo_program.to_account_info(),
        input_vault_mint: ctx.accounts.input_vault_mint.to_account_info(),
        output_vault_mint: ctx.accounts.output_vault_mint.to_account_info(),
    };

    let cpi_context = CpiContext::new(ctx.accounts.clmm_program.to_account_info(), cpi_accounts)
        .with_remaining_accounts(ctx.remaining_accounts.to_vec());

    // Use exact input with minimum output requirement
    cpi::swap_v2(
        cpi_context,
        frontrun_output, // Sell all tokens acquired in frontrun
        minimum_output,  // Ensure we get at least our minimum profitable amount
        if zero_for_one {
            // Set price limit to ensure the swap completes
            MIN_SQRT_PRICE_X64 + 1
        } else {
            MAX_SQRT_PRICE_X64 - 1
        },
        true, // Always base input for backrun - selling what we got
    )?;

    // Mark this sandwich as complete to prevent replay
    ctx.accounts.sandwich_state.is_complete = true;

    // Calculate and record profit
    ctx.accounts.output_token_account.reload()?;
    let actual_output = ctx
        .accounts
        .output_token_account
        .amount
        .checked_sub(output_token_balance_before)
        .unwrap();
    let profit = actual_output.saturating_sub(frontrun_input);

    // Emit profit event
    emit!(SandwichCompleteEvent {
        sandwich_id,
        profit,
        input_amount: frontrun_input,
        output_amount: actual_output,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

// Calculate slippage tolerance based on target parameters
#[allow(clippy::too_many_arguments)]
fn calculate_clmm_slippage(
    amount: u64,
    threshold: u64,
    is_base_input: bool,
    _sqrt_price_limit_x64: u128,
    current_sqrt_price_x64: u128,
    current_tick: i32,
    liquidity: u128,
    zero_for_one: bool,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    fund_fee_rate: u32,
) -> Result<u128> {
    if is_base_input {
        // For exact input, threshold is minimum output
        // Simulate expected output at current price
        let expected_output = simulate_clmm_swap_output(
            current_sqrt_price_x64,
            current_tick,
            liquidity,
            amount,
            zero_for_one,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )?;

        // Calculate slippage as (expected - threshold) / expected * 10000
        if expected_output > threshold {
            let slippage =
                ((expected_output - threshold) as u128 * 10000) / expected_output as u128;
            Ok(slippage)
        } else {
            // If threshold > expected, user expects more than current market offers
            // This could be due to using outdated price data or specific routing
            // Default to a small slippage to be safe
            Ok(10) // 0.1% slippage
        }
    } else {
        // For exact output, threshold is maximum input
        // Simulate expected input at current price
        let expected_input = simulate_clmm_swap_input(
            current_sqrt_price_x64,
            current_tick,
            liquidity,
            amount,
            zero_for_one,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )?;

        // Calculate slippage as (threshold - expected) / expected * 10000
        if threshold > expected_input {
            let slippage = ((threshold - expected_input) as u128 * 10000) / expected_input as u128;
            Ok(slippage)
        } else {
            // If expected > threshold, user is willing to pay less than current market requires
            // Default to a small slippage to be safe
            Ok(10) // 0.1% slippage
        }
    }
}

// Calculate expected price impact for a given amount
fn calculate_price_impact(
    current_sqrt_price_x64: u128,
    liquidity: u128,
    amount: u64,
    zero_for_one: bool,
    is_base_input: bool,
    fee_rate: u32,
) -> Result<u128> {
    // Adjust for fees
    let fee_adjustment = 1_000_000 - fee_rate as u128;
    let adjusted_amount = (amount as u128 * fee_adjustment) / 1_000_000;

    // Calculate the price impact based on the formula from the Uniswap/Raydium whitepaper
    if is_base_input {
        if zero_for_one {
            // Selling token 0 for token 1 - price goes down
            // Δsqrt(P) = -Δx * sqrt(P) / L
            let delta = (adjusted_amount * current_sqrt_price_x64) / liquidity;
            Ok(delta)
        } else {
            // Selling token 1 for token 0 - price goes up
            // Δsqrt(P) = Δy / L
            let delta = adjusted_amount * Q64 / liquidity;
            Ok(delta)
        }
    } else if zero_for_one {
        // Buying token 1 with token 0 - price goes down
        // Reverse calculate from output to input impact
        let delta = (adjusted_amount * Q64) / (liquidity * 2);
        Ok(delta)
    } else {
        // Buying token 0 with token 1 - price goes up
        // Reverse calculate from output to input impact
        let delta = (adjusted_amount * current_sqrt_price_x64) / (liquidity * 2);
        Ok(delta)
    }
}

// Calculate optimal sandwich amount using binary search
#[allow(clippy::too_many_arguments)]
fn calculate_optimal_clmm_sandwich_amount(
    current_sqrt_price_x64: u128,
    current_tick: i32,
    liquidity: u128,
    target_amount: u64,
    safe_slippage_bps: u128,
    target_is_base_input: bool,
    zero_for_one: bool,
    trade_fee_rate: u32,
    protocol_fee_rate: u32,
    fund_fee_rate: u32,
) -> Result<u64> {
    // Use binary search to find optimal attack size
    let max_search_amount = target_amount.saturating_mul(3);
    let mut low = 1u64;
    let mut high = max_search_amount;
    let mut best_amount = target_amount / 5; // Initial guess
    let mut best_profit = 0u64;

    // Binary search for up to 20 iterations to converge on optimal amount
    for _ in 0..20 {
        if low >= high {
            break;
        }

        let mid = (low + high) / 2;

        // 1. FRONTRUN: Calculate frontrun swap result
        let frontrun_price_impact = calculate_price_impact(
            current_sqrt_price_x64,
            liquidity,
            mid,
            zero_for_one,
            true, // Frontrun always uses exact input
            trade_fee_rate,
        )?;

        let after_frontrun_price = if zero_for_one {
            current_sqrt_price_x64.saturating_sub(frontrun_price_impact)
        } else {
            current_sqrt_price_x64.saturating_add(frontrun_price_impact)
        };

        // Simulate frontrun output
        let frontrun_output = simulate_clmm_swap_output(
            current_sqrt_price_x64,
            current_tick,
            liquidity,
            mid,
            zero_for_one,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )?;

        // 2. TARGET TX: Check if target would still succeed with new price
        // First, calculate target output or input before frontrun
        let (target_expected_output_before, target_expected_input_before) = if target_is_base_input
        {
            let output = simulate_clmm_swap_output(
                current_sqrt_price_x64,
                current_tick,
                liquidity,
                target_amount,
                zero_for_one,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            )?;
            (output, target_amount)
        } else {
            let input = simulate_clmm_swap_input(
                current_sqrt_price_x64,
                current_tick,
                liquidity,
                target_amount,
                zero_for_one,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            )?;
            (target_amount, input)
        };

        // Then, calculate target output after frontrun
        let (target_expected_output_after, target_expected_input_after) = if target_is_base_input {
            let output = simulate_clmm_swap_output(
                after_frontrun_price,
                current_tick, // Approximate, would need full tick crossing simulation
                liquidity,
                target_amount,
                zero_for_one,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            )?;
            (output, target_amount)
        } else {
            let input = simulate_clmm_swap_input(
                after_frontrun_price,
                current_tick,
                liquidity,
                target_amount,
                zero_for_one,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            )?;
            (target_amount, input)
        };

        // Calculate price impact percentage for target
        let price_impact_bps = if target_is_base_input {
            if target_expected_output_before > 0 {
                ((target_expected_output_before - target_expected_output_after) as u128 * 10000)
                    / target_expected_output_before as u128
            } else {
                0
            }
        } else if target_expected_input_before > 0 {
            ((target_expected_input_after - target_expected_input_before) as u128 * 10000)
                / target_expected_input_before as u128
        } else {
            0
        };

        // Check if target tx will still execute within slippage
        let within_slippage = price_impact_bps <= safe_slippage_bps;

        if !within_slippage {
            // If target tx would fail due to excessive slippage, reduce search space
            high = mid.saturating_sub(1);
            continue;
        }

        // 3. BACKRUN: Calculate result of selling frontrun output
        // First, predict the price after both frontrun and target tx
        let target_price_impact = if target_is_base_input {
            calculate_price_impact(
                after_frontrun_price,
                liquidity,
                target_amount,
                zero_for_one,
                true,
                trade_fee_rate,
            )?
        } else {
            // For exact output, we need to calculate the input amount first
            let target_input = simulate_clmm_swap_input(
                after_frontrun_price,
                current_tick,
                liquidity,
                target_amount,
                zero_for_one,
                trade_fee_rate,
                protocol_fee_rate,
                fund_fee_rate,
            )?;

            calculate_price_impact(
                after_frontrun_price,
                liquidity,
                target_input,
                zero_for_one,
                true,
                trade_fee_rate,
            )?
        };

        let after_target_price = if zero_for_one {
            after_frontrun_price.saturating_sub(target_price_impact)
        } else {
            after_frontrun_price.saturating_add(target_price_impact)
        };

        // Now calculate how much we'll get back in the backrun
        let backrun_output = simulate_clmm_swap_output(
            after_target_price,
            current_tick, // Approximate
            liquidity,
            frontrun_output,
            !zero_for_one, // Opposite direction from frontrun
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )?;

        // 4. Calculate profit and update if best so far
        let profit = backrun_output.saturating_sub(mid);

        if profit > best_profit {
            best_profit = profit;
            best_amount = mid;
        }

        // 5. Adjust search range based on profit trend
        if profit > 0 {
            // Profit is positive, try larger amounts
            low = mid + 1;
        } else {
            // Profit is negative or zero, try smaller amounts
            high = mid - 1;
        }
    }

    // Ensure we have a minimum viable amount
    if best_profit < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    Ok(best_amount)
}

// Simulate output amount for a CLMM swap
#[allow(clippy::too_many_arguments)]
fn simulate_clmm_swap_output(
    sqrt_price_x64: u128,
    _tick: i32,
    liquidity: u128,
    amount_in: u64,
    zero_for_one: bool,
    trade_fee_rate: u32,
    _protocol_fee_rate: u32,
    _fund_fee_rate: u32,
) -> Result<u64> {
    // Apply fee rate
    let fee_adjustment = 1_000_000 - trade_fee_rate as u128;
    let adjusted_amount = (amount_in as u128 * fee_adjustment) / 1_000_000;

    // Calculate output based on concentrated liquidity formulas
    let amount_out = if zero_for_one {
        // 0 -> 1, deltaY = L * (sqrt(P_b) - sqrt(P_a))
        // Here we estimate without full tick crossing calculations
        // This simplification doesn't account for liquidity changes across tick boundaries

        // Calculate new sqrt price
        let new_sqrt_price =
            sqrt_price_after_amount_in(sqrt_price_x64, liquidity, adjusted_amount, zero_for_one)?;

        // Calculate amount out using the formula
        let delta_y = if new_sqrt_price < sqrt_price_x64 {
            mul_div(liquidity, sqrt_price_x64 - new_sqrt_price, Q64)?
        } else {
            0
        };

        delta_y as u64
    } else {
        // 1 -> 0, deltaX = L * (1/sqrt(P_a) - 1/sqrt(P_b))
        // Convert to the form: deltaX = L * (sqrt(P_b) - sqrt(P_a)) / (sqrt(P_a) * sqrt(P_b))

        // Calculate new sqrt price
        let new_sqrt_price =
            sqrt_price_after_amount_in(sqrt_price_x64, liquidity, adjusted_amount, zero_for_one)?;

        // Calculate amount out using the formula
        let delta_x = if new_sqrt_price > sqrt_price_x64 {
            mul_div(liquidity, Q64, sqrt_price_x64)? - mul_div(liquidity, Q64, new_sqrt_price)?
        } else {
            0
        };

        delta_x as u64
    };

    Ok(amount_out)
}

// Simulate input amount required for a CLMM swap
#[allow(clippy::too_many_arguments)]
fn simulate_clmm_swap_input(
    sqrt_price_x64: u128,
    _tick: i32,
    liquidity: u128,
    amount_out: u64,
    zero_for_one: bool,
    trade_fee_rate: u32,
    _protocol_fee_rate: u32,
    _fund_fee_rate: u32,
) -> Result<u64> {
    // Calculate input based on concentrated liquidity formulas
    let raw_amount_in = if zero_for_one {
        // 0 -> 1, amount0 needed for amount1_out
        // We work backwards from the amount out formula
        let sqrt_price_delta = mul_div(amount_out as u128, Q64, liquidity)?;

        let new_sqrt_price = sqrt_price_x64.saturating_sub(sqrt_price_delta);

        // Calculate amount in needed to move the price to new_sqrt_price
        calculate_amount0_delta(
            sqrt_price_x64,
            new_sqrt_price,
            liquidity,
            true, // round up for input amount
        )?
    } else {
        // 1 -> 0, amount1 needed for amount0_out
        // We work backwards from the amount out formula
        let inv_sqrt_price_delta = mul_div(amount_out as u128, sqrt_price_x64, liquidity)?;

        let new_sqrt_price =
            sqrt_price_x64.saturating_add(mul_div(inv_sqrt_price_delta, Q64, sqrt_price_x64)?);

        // Calculate amount in needed to move the price to new_sqrt_price
        calculate_amount1_delta(
            sqrt_price_x64,
            new_sqrt_price,
            liquidity,
            true, // round up for input amount
        )?
    };

    // Apply fee rate to calculate total input required (raw_amount * 1_000_000 / (1_000_000 - fee_rate))
    let total_amount_in = mul_div(raw_amount_in, 1_000_000, 1_000_000 - trade_fee_rate as u128)?;

    Ok(total_amount_in as u64)
}

// Helper function to calculate sqrt price after an amount in
fn sqrt_price_after_amount_in(
    sqrt_price_x64: u128,
    liquidity: u128,
    amount_in: u128,
    zero_for_one: bool,
) -> Result<u128> {
    if zero_for_one {
        // 0 -> 1: sqrt(P) = L * sqrt(P0) / (L + amount_in * sqrt(P0))
        let product = mul_div(amount_in, sqrt_price_x64, Q64)?;

        let denominator = liquidity.saturating_add(product);

        if denominator == 0 {
            return err!(ErrorCode::CalculationFailure);
        }

        let new_sqrt_price = mul_div(liquidity, sqrt_price_x64, denominator)?;

        Ok(new_sqrt_price)
    } else {
        // 1 -> 0: sqrt(P) = sqrt(P0) + amount_in / L
        let sqrt_price_delta = mul_div(amount_in, Q64, liquidity)?;

        let new_sqrt_price = sqrt_price_x64.saturating_add(sqrt_price_delta);
        Ok(new_sqrt_price)
    }
}

// Helper function to calculate amount0 delta
fn calculate_amount0_delta(
    sqrt_price_a_x64: u128,
    sqrt_price_b_x64: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u128> {
    let (sqrt_price_low, sqrt_price_high) = if sqrt_price_a_x64 <= sqrt_price_b_x64 {
        (sqrt_price_a_x64, sqrt_price_b_x64)
    } else {
        (sqrt_price_b_x64, sqrt_price_a_x64)
    };

    let numerator1 = liquidity << 64;
    let numerator2 = sqrt_price_high - sqrt_price_low;

    if sqrt_price_low == 0 {
        return err!(ErrorCode::CalculationFailure);
    }

    let amount = if round_up {
        // Round up division for calculating input amounts
        mul_div_ceil(numerator1, numerator2, sqrt_price_high * sqrt_price_low)?
    } else {
        // Round down division for calculating output amounts
        mul_div(numerator1, numerator2, sqrt_price_high * sqrt_price_low)?
    };

    if sqrt_price_a_x64 <= sqrt_price_b_x64 {
        Ok(amount)
    } else {
        Ok(amount.neg())
    }
}

// Helper function to calculate amount1 delta
fn calculate_amount1_delta(
    sqrt_price_a_x64: u128,
    sqrt_price_b_x64: u128,
    liquidity: u128,
    round_up: bool,
) -> Result<u128> {
    let (sqrt_price_low, sqrt_price_high) = if sqrt_price_a_x64 <= sqrt_price_b_x64 {
        (sqrt_price_a_x64, sqrt_price_b_x64)
    } else {
        (sqrt_price_b_x64, sqrt_price_a_x64)
    };

    let amount = if round_up {
        // Round up division for calculating input amounts
        mul_div_ceil(liquidity, sqrt_price_high - sqrt_price_low, Q64)?
    } else {
        // Round down division for calculating output amounts
        mul_div(liquidity, sqrt_price_high - sqrt_price_low, Q64)?
    };

    if sqrt_price_a_x64 <= sqrt_price_b_x64 {
        Ok(amount)
    } else {
        Ok(amount.neg())
    }
}

// Helper for ceiling division
fn mul_div_ceil(a: u128, b: u128, denominator: u128) -> Result<u128> {
    let product = a.checked_mul(b).ok_or(ErrorCode::CalculationFailure)?;

    if product == 0 {
        return Ok(0);
    }

    let numerator = product - 1;
    let quotient = numerator / denominator;
    Ok(quotient + 1)
}

// Helper for floor division
fn mul_div(a: u128, b: u128, denominator: u128) -> Result<u128> {
    if denominator == 0 {
        return err!(ErrorCode::CalculationFailure);
    }

    let product = a.checked_mul(b).ok_or(ErrorCode::CalculationFailure)?;
    let result = product / denominator;

    Ok(result)
}

// Extension trait for negative value handling
trait Neg {
    fn neg(self) -> Self;
}

impl Neg for u128 {
    fn neg(self) -> Self {
        0u128.saturating_sub(self)
    }
}
pub fn get_recent_epoch() -> Result<u64> {
    Ok(Clock::get()?.epoch)
}

/// Calculate the fee for input amount
pub fn clmm_get_transfer_fee(
    mint_account: InterfaceAccount<Mint>,
    pre_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(get_recent_epoch()?, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}

/// Calculate the fee for output amount
pub fn clmm_get_transfer_inverse_fee(
    mint_account: InterfaceAccount<Mint>,
    post_fee_amount: u64,
) -> Result<u64> {
    let mint_info = mint_account.to_account_info();
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = get_recent_epoch()?;

        let transfer_fee = transfer_fee_config.get_epoch_fee(epoch);
        if u16::from(transfer_fee.transfer_fee_basis_points) == MAX_FEE_BASIS_POINTS {
            u64::from(transfer_fee.maximum_fee)
        } else {
            transfer_fee_config
                .calculate_inverse_epoch_fee(epoch, post_fee_amount)
                .unwrap()
        }
    } else {
        0
    };
    Ok(fee)
}
