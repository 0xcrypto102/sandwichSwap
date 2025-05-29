use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022::spl_token_2022::{
        self,
        extension::{
            transfer_fee::TransferFeeConfig, BaseStateWithExtensions, StateWithExtensions,
        },
    },
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use raydium_cpmm_cpi::{cpi, program::RaydiumCpmm};

use super::{CpmmAmmConfig, CpmmObservationState, CpmmPoolState};

use crate::error::ErrorCode;
use crate::sandwich_state::{SandwichCompleteEvent, SandwichState};
use super::CurveCalculator;

#[derive(Accounts)]
pub struct CpmmSwapBaseInput<'info> {
    pub cp_swap_program: Program<'info, RaydiumCpmm>,
    /// The user performing the swap
    pub payer: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
      seeds = [
        raydium_cpmm_cpi::AUTH_SEED.as_bytes(),
      ],
      seeds::program = cp_swap_program.key(),
      bump,
  )]
    pub authority: UncheckedAccount<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, CpmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, CpmmPoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(
      mut,
      constraint = input_vault.key() == pool_state.load()?.token_0_vault || input_vault.key() == pool_state.load()?.token_1_vault
  )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(
      mut,
      constraint = output_vault.key() == pool_state.load()?.token_0_vault || output_vault.key() == pool_state.load()?.token_1_vault
  )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL program for input token transfers
    pub input_token_program: Interface<'info, TokenInterface>,

    /// SPL program for output token transfers
    pub output_token_program: Interface<'info, TokenInterface>,

    /// The mint of input token
    #[account(
      address = input_vault.mint
  )]
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token
    #[account(
      address = output_vault.mint
  )]
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,
    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, CpmmObservationState>,
}

pub fn cpmm_swap_base_input(
    ctx: Context<CpmmSwapBaseInput>,
    amount_in: u64,
    minimum_amount_out: u64,
) -> Result<()> {
    let cpi_accounts = cpi::accounts::Swap {
        payer: ctx.accounts.payer.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        input_token_program: ctx.accounts.input_token_program.to_account_info(),
        output_token_program: ctx.accounts.output_token_program.to_account_info(),
        input_token_mint: ctx.accounts.input_token_mint.to_account_info(),
        output_token_mint: ctx.accounts.output_token_mint.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
    };
    let cpi_context = CpiContext::new(ctx.accounts.cp_swap_program.to_account_info(), cpi_accounts);
    cpi::swap_base_input(cpi_context, amount_in, minimum_amount_out)
}

#[derive(Accounts)]
#[instruction(sandwich_id: u64)]
pub struct CpmmSandwichFrontrun<'info> {
    pub cp_swap_program: Program<'info, RaydiumCpmm>,
    /// The user performing the swap
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
     seeds = [
       raydium_cpmm_cpi::AUTH_SEED.as_bytes(),
     ],
     seeds::program = cp_swap_program.key(),
     bump,
   )]
    pub authority: UncheckedAccount<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, CpmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, CpmmPoolState>,

    /// The user token account for input token
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token
    #[account(
     mut,
     constraint = input_vault.key() == pool_state.load()?.token_0_vault || input_vault.key() == pool_state.load()?.token_1_vault
   )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token
    #[account(
     mut,
     constraint = output_vault.key() == pool_state.load()?.token_0_vault || output_vault.key() == pool_state.load()?.token_1_vault
   )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL program for input token transfers
    pub input_token_program: Interface<'info, TokenInterface>,

    /// SPL program for output token transfers
    pub output_token_program: Interface<'info, TokenInterface>,

    /// The mint of input token
    #[account(address = input_vault.mint)]
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token
    #[account(address = output_vault.mint)]
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, CpmmObservationState>,

    /// The account that will store sandwich state
    #[account(
       init,
       payer = payer,
       space = 8 + SandwichState::SIZE,
       seeds = [b"sandwich", &sandwich_id.to_le_bytes()],
       bump
   )]
    pub sandwich_state: Account<'info, SandwichState>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(sandwich_id: u64)]
pub struct CpmmSandwichBackrun<'info> {
    pub cp_swap_program: Program<'info, RaydiumCpmm>,
    /// The user performing the swap
    #[account(mut)]
    pub payer: Signer<'info>,

    /// CHECK: pool vault and lp mint authority
    #[account(
     seeds = [
       raydium_cpmm_cpi::AUTH_SEED.as_bytes(),
     ],
     seeds::program = cp_swap_program.key(),
     bump,
   )]
    pub authority: UncheckedAccount<'info>,

    /// The factory state to read protocol fees
    #[account(address = pool_state.load()?.amm_config)]
    pub amm_config: Box<Account<'info, CpmmAmmConfig>>,

    /// The program account of the pool in which the swap will be performed
    #[account(mut)]
    pub pool_state: AccountLoader<'info, CpmmPoolState>,

    /// The user token account for input token (was output in frontrun)
    #[account(mut)]
    pub input_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The user token account for output token (was input in frontrun)
    #[account(mut)]
    pub output_token_account: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for input token (was output in frontrun)
    #[account(
     mut,
     constraint = input_vault.key() == pool_state.load()?.token_0_vault || input_vault.key() == pool_state.load()?.token_1_vault
   )]
    pub input_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// The vault token account for output token (was input in frontrun)
    #[account(
     mut,
     constraint = output_vault.key() == pool_state.load()?.token_0_vault || output_vault.key() == pool_state.load()?.token_1_vault
   )]
    pub output_vault: Box<InterfaceAccount<'info, TokenAccount>>,

    /// SPL program for input token transfers
    pub input_token_program: Interface<'info, TokenInterface>,

    /// SPL program for output token transfers
    pub output_token_program: Interface<'info, TokenInterface>,

    /// The mint of input token (was output in frontrun)
    #[account(address = input_vault.mint)]
    pub input_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The mint of output token (was input in frontrun)
    #[account(address = output_vault.mint)]
    pub output_token_mint: Box<InterfaceAccount<'info, Mint>>,

    /// The program account for the most recent oracle observation
    #[account(mut, address = pool_state.load()?.observation_key)]
    pub observation_state: AccountLoader<'info, CpmmObservationState>,

    /// The account that stores sandwich state
    #[account(
       mut,
       seeds = [b"sandwich", &sandwich_id.to_le_bytes()],
       bump = sandwich_state.bump,
       constraint = !sandwich_state.is_complete @ ErrorCode::SandwichAlreadyCompleted,
       constraint = sandwich_state.token_in_mint == *output_token_mint.to_account_info().key
           @ ErrorCode::TokenMintMismatch,
       constraint = sandwich_state.token_out_mint == *input_token_mint.to_account_info().key
           @ ErrorCode::TokenMintMismatch
   )]
    pub sandwich_state: Account<'info, SandwichState>,
}

pub fn cpmm_frontrun_swap_base_input(
    ctx: Context<CpmmSandwichFrontrun>,
    target_amount_in: u64,
    target_minimum_amount_out: u64,
    sandwich_id: u64,
) -> Result<()> {
    // Load the pool state to access current reserves
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;

    // Determine trade direction and get current reserves
    let (_trade_direction, total_input_amount, total_output_amount) =
        if ctx.accounts.input_vault.key() == pool_state.token_0_vault
            && ctx.accounts.output_vault.key() == pool_state.token_1_vault
        {
            let (input_amount, output_amount) = vault_amount_without_fee(
                pool_state,
                ctx.accounts.input_vault.amount,
                ctx.accounts.output_vault.amount,
            );
            (0, input_amount, output_amount) // ZeroForOne
        } else if ctx.accounts.input_vault.key() == pool_state.token_1_vault
            && ctx.accounts.output_vault.key() == pool_state.token_0_vault
        {
            let (output_amount, input_amount) = vault_amount_without_fee(
                pool_state,
                ctx.accounts.output_vault.amount,
                ctx.accounts.input_vault.amount,
            );
            (1, input_amount, output_amount) // OneForZero
        } else {
            return err!(ErrorCode::InvalidVault);
        };

    // Calculate input transfer fee for target transaction
    let target_transfer_fee = get_transfer_fee(
        &ctx.accounts.input_token_mint.to_account_info(),
        target_amount_in,
    )?;
    let target_actual_amount_in = target_amount_in.saturating_sub(target_transfer_fee);

    // Calculate expected output for the target transaction at current state
    let expected_target_output = calculate_expected_output(
        target_actual_amount_in,
        total_input_amount,
        total_output_amount,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Calculate target slippage tolerance
    let target_slippage_bps = if expected_target_output > 0 {
        // Calculate as basis points (10000 = 100%)
        ((expected_target_output.saturating_sub(target_minimum_amount_out)) as u128 * 10000)
            / (expected_target_output as u128)
    } else {
        return err!(ErrorCode::CalculationFailure);
    };

    // Calculate maximum price impact we can cause
    // We want to stay just below target's slippage threshold (95% of their tolerance)
    let safe_slippage_bps = target_slippage_bps.saturating_mul(95).saturating_div(100);

    // Calculate optimal sandwich buy amount with improved profit calculation
    let optimal_buy_amount = calculate_optimal_sandwich_amount(
        total_input_amount,
        total_output_amount,
        safe_slippage_bps,
        target_amount_in,
        target_actual_amount_in,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Ensure calculated amount is reasonable
    if optimal_buy_amount < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    // Record initial output token balance
    let output_token_balance_before = ctx.accounts.output_token_account.amount;

    // Execute the buy transaction with calculated amount
    let cpi_accounts = cpi::accounts::Swap {
        payer: ctx.accounts.payer.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        input_token_program: ctx.accounts.input_token_program.to_account_info(),
        output_token_program: ctx.accounts.output_token_program.to_account_info(),
        input_token_mint: ctx.accounts.input_token_mint.to_account_info(),
        output_token_mint: ctx.accounts.output_token_mint.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
    };

    // Calculate minimum amount out for our sandwich buy
    // We use a more aggressive slippage for our transaction to ensure it goes through
    let minimum_out_for_sandwich = calculate_minimum_out_for_sandwich(
        optimal_buy_amount,
        total_input_amount,
        total_output_amount,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Execute the CPI call to perform the swap
    let cpi_context = CpiContext::new(ctx.accounts.cp_swap_program.to_account_info(), cpi_accounts);
    cpi::swap_base_input(cpi_context, optimal_buy_amount, minimum_out_for_sandwich)?;

    // Calculate actual frontrun output amount
    let output_token_balance_after = ctx.accounts.output_token_account.amount;
    let frontrun_output_amount =
        output_token_balance_after.saturating_sub(output_token_balance_before);

    // Store frontrun data in the PDA for the backrun to read
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = frontrun_output_amount;
    sandwich_state.frontrun_input_amount = optimal_buy_amount;
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_in_mint = *ctx.accounts.input_token_mint.to_account_info().key;
    sandwich_state.token_out_mint = *ctx.accounts.output_token_mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;

    Ok(())
}

pub fn cpmm_backrun_swap_base_input(
    ctx: Context<CpmmSandwichBackrun>,
    sandwich_id: u64,
) -> Result<()> {
    // Get the exact amount from the frontrun transaction
    let frontrun_output = ctx.accounts.sandwich_state.frontrun_output_amount;
    let frontrun_input = ctx.accounts.sandwich_state.frontrun_input_amount;

    // Load pool state to get current reserves (after target tx)
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;

    // Determine trade direction and get current reserves
    let (_trade_direction, current_input_amount, current_output_amount) =
        if ctx.accounts.input_vault.key() == pool_state.token_0_vault
            && ctx.accounts.output_vault.key() == pool_state.token_1_vault
        {
            let (input_amount, output_amount) = vault_amount_without_fee(
                pool_state,
                ctx.accounts.input_vault.amount,
                ctx.accounts.output_vault.amount,
            );
            (0, input_amount, output_amount) // ZeroForOne
        } else if ctx.accounts.input_vault.key() == pool_state.token_1_vault
            && ctx.accounts.output_vault.key() == pool_state.token_0_vault
        {
            let (output_amount, input_amount) = vault_amount_without_fee(
                pool_state,
                ctx.accounts.output_vault.amount,
                ctx.accounts.input_vault.amount,
            );
            (1, input_amount, output_amount) // OneForZero
        } else {
            return err!(ErrorCode::InvalidVault);
        };

    // Calculate expected output from backrun based on current reserves
    let expected_backrun_output = calculate_expected_output(
        frontrun_output,
        current_input_amount,
        current_output_amount,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Verify that the backrun would be profitable (return more than we put in)
    let min_profit_factor = 1005; // 0.5% minimum profit
    let min_required_output = frontrun_input
        .checked_mul(min_profit_factor)
        .ok_or(ErrorCode::CalculationFailure)?
        .checked_div(1000)
        .ok_or(ErrorCode::CalculationFailure)?;

    // Use the higher of expected output with safety margin or minimum required output
    let minimum_backrun_output = std::cmp::max(
        expected_backrun_output
            .saturating_mul(98)
            .saturating_div(100), // 2% safety margin
        min_required_output,
    );

    // Verify potential profitability
    if minimum_backrun_output <= frontrun_input {
        return err!(ErrorCode::UnprofitableSandwich);
    }

    // Record initial token balance for profit calculation
    let output_token_balance_before = ctx.accounts.output_token_account.amount;

    // Execute the backrun swap (selling tokens acquired in frontrun)
    let cpi_accounts = cpi::accounts::Swap {
        payer: ctx.accounts.payer.to_account_info(),
        authority: ctx.accounts.authority.to_account_info(),
        amm_config: ctx.accounts.amm_config.to_account_info(),
        pool_state: ctx.accounts.pool_state.to_account_info(),
        input_token_account: ctx.accounts.input_token_account.to_account_info(),
        output_token_account: ctx.accounts.output_token_account.to_account_info(),
        input_vault: ctx.accounts.input_vault.to_account_info(),
        output_vault: ctx.accounts.output_vault.to_account_info(),
        input_token_program: ctx.accounts.input_token_program.to_account_info(),
        output_token_program: ctx.accounts.output_token_program.to_account_info(),
        input_token_mint: ctx.accounts.input_token_mint.to_account_info(),
        output_token_mint: ctx.accounts.output_token_mint.to_account_info(),
        observation_state: ctx.accounts.observation_state.to_account_info(),
    };

    let cpi_context = CpiContext::new(ctx.accounts.cp_swap_program.to_account_info(), cpi_accounts);
    cpi::swap_base_input(cpi_context, frontrun_output, minimum_backrun_output)?;

    // Mark this sandwich as complete to prevent replay
    ctx.accounts.sandwich_state.is_complete = true;

    // Calculate and store actual profit
    let output_token_balance_after = ctx.accounts.output_token_account.amount;
    let actual_output = output_token_balance_after.saturating_sub(output_token_balance_before);
    let profit = actual_output.saturating_sub(frontrun_input);

    // Emit an event with profit information
    emit!(SandwichCompleteEvent {
        sandwich_id,
        profit,
        input_amount: frontrun_input,
        output_amount: actual_output,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

// this is from the raydium cpmm code
// https://github.com/raydium-io/raydium-cp-swap/blob/183ddbb11550cea212710a98351779a41873258b/programs/cp-swap/src/states/pool.rs#L142
pub fn vault_amount_without_fee(
    cpmm_pool_state: &CpmmPoolState,
    vault_0: u64,
    vault_1: u64,
) -> (u64, u64) {
    (
        vault_0
            .checked_sub(cpmm_pool_state.protocol_fees_token_0 + cpmm_pool_state.fund_fees_token_0)
            .unwrap(),
        vault_1
            .checked_sub(cpmm_pool_state.protocol_fees_token_1 + cpmm_pool_state.fund_fees_token_1)
            .unwrap(),
    )
}

// Helper function to calculate expected output amount
fn calculate_expected_output(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Use Raydium's CurveCalculator to calculate the expected output
    let result = CurveCalculator::swap_base_input(
        amount_in.into(),
        reserve_in.into(),
        reserve_out.into(),
        trade_fee_rate,
        protocol_fee_rate,
        fund_fee_rate,
    )
    .ok_or(ErrorCode::CalculationFailure)?;

    // Extract the destination_amount_swapped from the result
    let amount_out = result.destination_amount_swapped;

    // Convert back to u64
    Ok(amount_out
        .try_into()
        .map_err(|_| ErrorCode::CalculationFailure)?)
}

// Calculate the optimal amount to buy for sandwich attack with full sandwich simulation
#[allow(clippy::too_many_arguments)]
fn calculate_optimal_sandwich_amount(
    reserve_in: u64,
    reserve_out: u64,
    safe_slippage_bps: u128,
    _target_amount_in: u64,
    target_actual_amount_in: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Convert to u128 for safer math
    let reserve_in = reserve_in as u128;
    let reserve_out = reserve_out as u128;
    let target_amount_in = target_actual_amount_in as u128;

    // Initial estimate and binary search setup
    let initial_estimate = reserve_in.checked_div(100).unwrap_or(1000);
    let max_amount = reserve_in.checked_div(10).unwrap_or(reserve_in);

    let mut low = 1u128;
    let mut high = max_amount;
    let mut best_amount = initial_estimate;
    let mut best_profit = 0u128;

    // Binary search to find optimal amount
    for _ in 0..20 {
        if low >= high {
            break;
        }
        let mid = (low + high) / 2;

        // 1. FRONTRUN: Calculate outcome of frontrun transaction
        let frontrun_result = CurveCalculator::swap_base_input(
            mid, // trial amount for frontrun
            reserve_in,
            reserve_out,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?;

        // Get frontrun output amount and new reserves after frontrun
        let frontrun_output_amount = frontrun_result.destination_amount_swapped;
        let new_reserve_in = reserve_in + frontrun_result.source_amount_swapped;
        let new_reserve_out = reserve_out - frontrun_output_amount;

        // 2. TARGET TX: Simulate target transaction on new reserves
        // First calculate if this still allows target tx to succeed within slippage
        let target_expected_output_before = CurveCalculator::swap_base_input(
            target_amount_in,
            reserve_in,  // original reserve
            reserve_out, // original reserve
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?
        .destination_amount_swapped;

        let target_expected_output_after = CurveCalculator::swap_base_input(
            target_amount_in,
            new_reserve_in,  // after frontrun
            new_reserve_out, // after frontrun
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?
        .destination_amount_swapped;

        // Check if target tx will still execute within slippage
        let price_impact_bps = ((target_expected_output_before - target_expected_output_after)
            * 10000)
            / target_expected_output_before;

        let within_slippage = price_impact_bps <= safe_slippage_bps;

        // If target would fail due to slippage, this attack size doesn't work
        if !within_slippage {
            high = mid - 1;
            continue;
        }

        // 3. Calculate state after target tx executes
        let after_target_reserve_in = new_reserve_in + target_amount_in;
        let after_target_reserve_out = new_reserve_out - target_expected_output_after;

        // 4. BACKRUN: Calculate result of selling frontrun_output_amount
        let backrun_result = CurveCalculator::swap_base_input(
            frontrun_output_amount,   // selling what we got in frontrun
            after_target_reserve_out, // using reserves after target tx
            after_target_reserve_in,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?;

        // 5. Calculate actual profit (what we get back minus what we put in)
        let backrun_output_amount = backrun_result.destination_amount_swapped;
        let profit = backrun_output_amount.saturating_sub(mid);

        // 6. Update best if this is more profitable
        if profit > best_profit {
            best_profit = profit;
            best_amount = mid;
        }

        // 7. Adjust search range - try larger amounts if profitable
        if profit > 0 {
            low = mid + 1;
        } else {
            high = mid - 1;
        }
    }

    // Convert best amount to u64 and return
    let result = best_amount.try_into().unwrap_or(u64::MAX);

    Ok(result)
}

// Calculate minimum output amount for our sandwich buy with aggressive slippage
fn calculate_minimum_out_for_sandwich(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Calculate expected output
    let expected_out = calculate_expected_output(
        amount_in,
        reserve_in,
        reserve_out,
        trade_fee_rate,
        protocol_fee_rate,
        fund_fee_rate,
    )?;

    // Apply aggressive slippage tolerance (5%)
    let min_out = expected_out.saturating_mul(95).saturating_div(100);

    Ok(min_out)
}

// Calculates any transfer fees associated with the target’s input token and
// subtracts them to determine the actual amount that affects the pool.
// this is from the raydium cpmm code
// https://github.com/raydium-io/raydium-cp-swap/blob/183ddbb11550cea212710a98351779a41873258b/programs/cp-swap/src/utils/token.rs#L159
pub fn get_transfer_fee(mint_info: &AccountInfo, pre_fee_amount: u64) -> Result<u64> {
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        transfer_fee_config
            .calculate_epoch_fee(Clock::get()?.epoch, pre_fee_amount)
            .unwrap()
    } else {
        0
    };
    Ok(fee)
}
