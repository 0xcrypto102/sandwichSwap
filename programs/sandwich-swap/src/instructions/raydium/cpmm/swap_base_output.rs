use crate::{error::ErrorCode};
use anchor_lang::prelude::*;
use anchor_spl::{
    token::Token,
    token_2022::spl_token_2022::{
        self,
        extension::{
            transfer_fee::{TransferFeeConfig, MAX_FEE_BASIS_POINTS},
            BaseStateWithExtensions, StateWithExtensions,
        },
    },
    token_interface::{Mint, TokenAccount, TokenInterface},
};
use raydium_cpmm_cpi::{cpi, program::RaydiumCpmm};
use crate::sandwich_state::{SandwichCompleteEvent, SandwichState};
use super::{vault_amount_without_fee, CurveCalculator};
use super::{CpmmAmmConfig, CpmmObservationState, CpmmPoolState};

#[derive(Accounts)]
pub struct CpmmSwapBaseOutput<'info> {
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

pub fn cpmm_swap_base_output(
    ctx: Context<CpmmSwapBaseOutput>,
    max_amount_in: u64,
    amount_out: u64,
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
    cpi::swap_base_output(cpi_context, max_amount_in, amount_out)
}

#[derive(Accounts)]
#[instruction(sandwich_id: u64)]
pub struct CpmmSandwichFrontrunOutput<'info> {
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
pub struct CpmmSandwichBackrunOutput<'info> {
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

pub fn cpmm_frontrun_swap_base_output(
    ctx: Context<CpmmSandwichFrontrunOutput>,
    target_max_amount_in: u64,
    target_amount_out: u64,
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

    // For swap_base_output, we need to calculate how much input will be required
    // for the target's requested output amount
    let out_transfer_fee = get_transfer_inverse_fee(
        &ctx.accounts.output_token_mint.to_account_info(),
        target_amount_out,
    )?;
    let target_actual_amount_out = target_amount_out.checked_add(out_transfer_fee).unwrap();

    // Calculate how much input the target will need to provide for their requested output
    let target_swap_result = CurveCalculator::swap_base_output(
        u128::from(target_actual_amount_out),
        u128::from(total_input_amount),
        u128::from(total_output_amount),
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )
    .ok_or(ErrorCode::CalculationFailure)?;

    let target_source_amount = target_swap_result.source_amount_swapped;
    let target_transfer_fee = get_transfer_inverse_fee(
        &ctx.accounts.input_token_mint.to_account_info(),
        u64::try_from(target_source_amount).unwrap(),
    )?;
    let target_actual_amount_in = u64::try_from(target_source_amount)
        .unwrap()
        .checked_add(target_transfer_fee)
        .unwrap();

    // Calculate target's slippage tolerance
    // Target's max_amount_in represents the maximum they're willing to pay
    let target_slippage_bps = if target_actual_amount_in > 0 {
        // Calculate as basis points (10000 = 100%)
        ((target_max_amount_in.saturating_sub(target_actual_amount_in)) as u128 * 10000)
            / (target_actual_amount_in as u128)
    } else {
        return err!(ErrorCode::CalculationFailure);
    };

    // Calculate maximum price impact we can cause
    // We want to stay just below target's slippage threshold (95% of their tolerance)
    let safe_slippage_bps = target_slippage_bps.saturating_mul(95).saturating_div(100);

    // Calculate optimal sandwich buy output amount
    // For output swaps, we want to reduce the output token reserves
    // to make the target have to pay more input tokens
    let optimal_output_amount = calculate_optimal_sandwich_output_amount(
        total_input_amount,
        total_output_amount,
        safe_slippage_bps,
        target_actual_amount_out,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Ensure calculated amount is reasonable
    if optimal_output_amount < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    // Record initial output token balance
    let output_token_balance_before = ctx.accounts.output_token_account.amount;

    // Calculate maximum amount in for our sandwich buy
    // We use a more aggressive slippage for our transaction to ensure it goes through
    let max_in_for_sandwich = calculate_max_input_for_sandwich(
        optimal_output_amount,
        total_input_amount,
        total_output_amount,
        ctx.accounts.amm_config.trade_fee_rate,
        ctx.accounts.amm_config.protocol_fee_rate,
        ctx.accounts.amm_config.fund_fee_rate,
    )?;

    // Execute the CPI call to perform the swap
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
    cpi::swap_base_output(cpi_context, max_in_for_sandwich, optimal_output_amount)?;

    // Calculate actual frontrun input and output amounts
    let output_token_balance_after = ctx.accounts.output_token_account.amount;
    let frontrun_output_amount =
        output_token_balance_after.saturating_sub(output_token_balance_before);

    // Store frontrun data in the PDA for the backrun to read
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = frontrun_output_amount;
    sandwich_state.frontrun_input_amount = max_in_for_sandwich; // We use max_in as the actual amount could be lower
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_in_mint = *ctx.accounts.input_token_mint.to_account_info().key;
    sandwich_state.token_out_mint = *ctx.accounts.output_token_mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;

    Ok(())
}

pub fn cpmm_backrun_swap_base_output(
    ctx: Context<CpmmSandwichBackrunOutput>,
    sandwich_id: u64,
) -> Result<()> {
    // Get the exact amount from the frontrun transaction
    let frontrun_output = ctx.accounts.sandwich_state.frontrun_output_amount;
    let frontrun_input = ctx.accounts.sandwich_state.frontrun_input_amount;

    // Load pool state to get current reserves (after target tx)
    let pool_state = &mut ctx.accounts.pool_state.load_mut()?;

    // Determine trade direction and get current reserves
    let (_trade_direction, _current_input_amount, _current_output_amount) =
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

    // For the backrun in an output-based sandwich, we want to get back at least what we spent
    // plus a minimum profit margin
    let min_profit_factor = 1005; // 0.5% minimum profit
    let min_amount_out = frontrun_input
        .checked_mul(min_profit_factor)
        .ok_or(ErrorCode::CalculationFailure)?
        .checked_div(1000)
        .ok_or(ErrorCode::CalculationFailure)?;

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

    // For the backrun, we're doing a base_output swap - we specify exactly how much of the output token we want
    // (which should be more than we put in for frontrun to make a profit)
    let cpi_context = CpiContext::new(ctx.accounts.cp_swap_program.to_account_info(), cpi_accounts);

    // Calculate maximum input needed (frontrun tokens plus a safety margin)
    let max_input_for_backrun = frontrun_output.saturating_mul(105).saturating_div(100); // 5% safety margin

    // Execute the swap - specify how much we want back, and the max we're willing to pay
    cpi::swap_base_output(cpi_context, max_input_for_backrun, min_amount_out)?;

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

// Calculate the optimal amount of output tokens to buy for sandwich attack on base output swaps
// simulates full sandwich
fn calculate_optimal_sandwich_output_amount(
    reserve_in: u64,
    reserve_out: u64,
    safe_slippage_bps: u128,
    target_amount_out: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Convert to u128 for safer math
    let reserve_in = reserve_in as u128;
    let reserve_out = reserve_out as u128;
    let target_amount_out = target_amount_out as u128;

    // Initial estimate for optimal output amount (can be refined)
    // Using 1% of reserve as starting point
    let initial_estimate = reserve_out.checked_div(100).unwrap_or(1000);
    let max_amount = reserve_out.checked_div(10).unwrap_or(reserve_out); // 10% of pool as upper limit

    // Binary search to find optimal output amount
    let mut low = 1u128; // Start with minimum meaningful amount
    let mut high = max_amount;
    let mut best_amount = initial_estimate;
    let mut best_profit = 0u128;

    // Limit iterations to prevent infinite loops
    for _ in 0..20 {
        if low >= high {
            break;
        }

        let mid = (low + high) / 2;

        // 1. FRONTRUN: Calculate outcome of our buy transaction (swap_base_output)
        let buy_result = CurveCalculator::swap_base_output(
            mid, // amount of output token we want
            reserve_in,
            reserve_out,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?;

        // Calculate new reserves after our buy
        let new_reserve_in = reserve_in + buy_result.source_amount_swapped;
        let new_reserve_out = reserve_out - buy_result.destination_amount_swapped;

        // 2. TARGET TX: Simulate target transaction on new reserves
        // First calculate how much input will be required for target's desired output with original reserves
        let target_expected_input_before = CurveCalculator::swap_base_output(
            target_amount_out,
            reserve_in,  // original reserve
            reserve_out, // original reserve
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?
        .source_amount_swapped;

        // Then calculate how much input will be required after our frontrun
        let target_expected_input_after = CurveCalculator::swap_base_output(
            target_amount_out,
            new_reserve_in,  // after frontrun
            new_reserve_out, // after frontrun
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?
        .source_amount_swapped;

        // Check if target tx will still execute within slippage
        let price_impact_bps = ((target_expected_input_after - target_expected_input_before)
            * 10000)
            / target_expected_input_before;

        let within_slippage = price_impact_bps <= safe_slippage_bps;

        // If target would fail due to slippage, this attack size doesn't work
        if !within_slippage {
            high = mid - 1;
            continue;
        }

        // 3. Calculate state after target tx executes
        let after_target_reserve_in = new_reserve_in + target_expected_input_after;
        let after_target_reserve_out = new_reserve_out - target_amount_out;

        // 4. BACKRUN: Calculate result of selling frontrun_output_amount (mid)
        // For backrun, we want to get back more than we put in (buy_result.source_amount_swapped)
        let backrun_result = CurveCalculator::swap_base_output(
            buy_result
                .source_amount_swapped
                .checked_mul(101)
                .unwrap_or(buy_result.source_amount_swapped)
                / 100, // Aim for 1% profit minimum
            after_target_reserve_out, // using reserves after target tx
            after_target_reserve_in,
            trade_fee_rate,
            protocol_fee_rate,
            fund_fee_rate,
        )
        .ok_or(ErrorCode::CalculationFailure)?;

        // 5. Calculate actual profit (what we get back minus what we put in)
        let our_cost = buy_result.source_amount_swapped;
        let our_return = backrun_result.destination_amount_swapped;
        let profit = our_return.saturating_sub(our_cost);

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

    // Convert best amount to u64
    let result = best_amount.try_into().unwrap_or(u64::MAX);

    // Ensure we have a minimum viable amount
    if result < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    Ok(result)
}

// Calculate maximum input amount for our sandwich buy with aggressive slippage
fn calculate_max_input_for_sandwich(
    amount_out: u64,
    reserve_in: u64,
    reserve_out: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Calculate expected input needed
    let swap_result = CurveCalculator::swap_base_output(
        u128::from(amount_out),
        u128::from(reserve_in),
        u128::from(reserve_out),
        trade_fee_rate,
        protocol_fee_rate,
        fund_fee_rate,
    )
    .ok_or(ErrorCode::CalculationFailure)?;

    let expected_in = u64::try_from(swap_result.source_amount_swapped).unwrap();

    // Apply aggressive slippage tolerance (5% more than calculated amount)
    let max_in = expected_in.saturating_mul(105).saturating_div(100);

    Ok(max_in)
}

// this is from the raydium cpmm code
// https://github.com/raydium-io/raydium-cp-swap/blob/183ddbb11550cea212710a98351779a41873258b/programs/cp-swap/src/utils/token.rs#L131
/// Calculate the fee for output amount
pub fn get_transfer_inverse_fee(mint_info: &AccountInfo, post_fee_amount: u64) -> Result<u64> {
    if *mint_info.owner == Token::id() {
        return Ok(0);
    }
    if post_fee_amount == 0 {
        return err!(ErrorCode::InvalidInput);
    }
    let mint_data = mint_info.try_borrow_data()?;
    let mint = StateWithExtensions::<spl_token_2022::state::Mint>::unpack(&mint_data)?;

    let fee = if let Ok(transfer_fee_config) = mint.get_extension::<TransferFeeConfig>() {
        let epoch = Clock::get()?.epoch;

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
