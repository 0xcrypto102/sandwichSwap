use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::Instruction, program::invoke_signed};
use anchor_spl::associated_token::get_associated_token_address;

use crate::instructions::{get_transfer_fee, CurveCalculator, Fees};
use crate::error::ErrorCode;

use super::PumpSwapContext;

// Sell instruction data structure
#[derive(AnchorSerialize)]
pub struct PumpSwapSell {
    pub base_amount_in: u64,
    pub min_quote_amount_out: u64,
}

impl PumpSwapSell {
    pub fn data(&self) -> Vec<u8> {
        let mut data = vec![51, 230, 133, 164, 1, 127, 131, 173]; // sell instruction discriminator
        data.extend_from_slice(&self.base_amount_in.to_le_bytes());
        data.extend_from_slice(&self.min_quote_amount_out.to_le_bytes());
        data
    }
}

pub fn pumpswap_frontrun_sell(
    ctx: Context<PumpSwapContext>,
    base_amount_in: u64,
    min_quote_amount_out: u64,
    sandwich_id: u64,
) -> Result<()> {
    // Get accounts needed for the CPI
    let pump_program = ctx.accounts.pump_amm_program.to_account_info();
    let pool = ctx.accounts.pool.to_account_info();
    let user = ctx.accounts.user.to_account_info();
    let global_config = ctx.accounts.global_config.to_account_info();
    let base_mint = ctx.accounts.base_mint.to_account_info();
    let quote_mint = ctx.accounts.quote_mint.to_account_info();
    let user_base_token_account = ctx.accounts.user_base_token_account.to_account_info();
    let user_quote_token_account = ctx.accounts.user_quote_token_account.to_account_info();
    let pool_base_token_account = ctx.accounts.pool_base_token_account.to_account_info();
    let pool_quote_token_account = ctx.accounts.pool_quote_token_account.to_account_info();
    let protocol_fee_recipient = ctx.accounts.protocol_fee_recipient.to_account_info();
    let protocol_fee_recipient_token_account = ctx
        .accounts
        .protocol_fee_recipient_token_account
        .to_account_info();
    let base_token_program = ctx.accounts.base_token_program.to_account_info();
    let quote_token_program = ctx.accounts.quote_token_program.to_account_info();
    let system_program = ctx.accounts.system_program.to_account_info();
    let associated_token_program = ctx.accounts.associated_token_program.to_account_info();
    let event_authority = ctx.accounts.event_authority.to_account_info();
    let program = ctx.accounts.program.to_account_info();
    
    let pool_state = &mut ctx.accounts.pool.load_mut()?;
    
    // Determine trade direction and get current reserves
    let (_trade_direction, total_input_amount, total_output_amount) =
        if ctx.accounts.pool_base_token_account.key() == get_associated_token_address(&*pool.key, &pool_state.base_mint)
            && ctx.accounts.pool_quote_token_account.key() == get_associated_token_address(&*pool.key, &pool_state.quote_mint)
        {
            let (input_amount, output_amount) = vault_amount_without_fee(
                ctx.accounts.pool_base_token_account.amount,
                ctx.accounts.pool_quote_token_account.amount,
            );
            (0, input_amount, output_amount) // ZeroForOne
        } else if ctx.accounts.pool_quote_token_account.key() == get_associated_token_address(&*pool.key, &pool_state.base_mint)
            && ctx.accounts.pool_base_token_account.key() == get_associated_token_address(&*pool.key, &pool_state.quote_mint)
        {
            let (output_amount, input_amount) = vault_amount_without_fee(
                ctx.accounts.pool_quote_token_account.amount,
                ctx.accounts.pool_base_token_account.amount,
            );
            (1, input_amount, output_amount) // OneForZero
        } else {
            return err!(ErrorCode::InvalidVault);
        };
    
    let target_transfer_fee = get_transfer_fee(
        &ctx.accounts.pool_base_token_account.to_account_info(),
        base_amount_in,
    )?;
    let target_actual_amount_in = base_amount_in.saturating_sub(target_transfer_fee);
    let global_config_data = ctx.accounts.global_config.load()?;
    
    let expected_target_output = calculate_expected_output(
        target_actual_amount_in,
        total_input_amount,
        total_output_amount,
        global_config_data.coin_creator_fee_basis_points * 100u64,
        global_config_data.protocol_fee_basis_points * 100u64,
        global_config_data.lp_fee_basis_points * 100u64,
    )?;
    
    let target_slippage_bps = if expected_target_output > 0 {
        // Calculate as basis points (10000 = 100%)
        ((expected_target_output.saturating_sub(min_quote_amount_out)) as u128 * 10000)
            / (expected_target_output as u128)
    } else {
        return err!(ErrorCode::CalculationFailure);
    };
    
    let safe_slippage_bps = target_slippage_bps.saturating_mul(95).saturating_div(100);

    // Calculate optimal sandwich buy amount with improved profit calculation
    let optimal_buy_amount = calculate_optimal_sandwich_amount(
        total_input_amount,
        total_output_amount,
        safe_slippage_bps,
        base_amount_in,
        target_actual_amount_in,
        global_config_data.coin_creator_fee_basis_points * 100u64,
        global_config_data.protocol_fee_basis_points * 100u64,
        global_config_data.lp_fee_basis_points * 100u64,
    )?;
    
    if optimal_buy_amount < 100 {
        return err!(ErrorCode::InsufficientSandwichAmount);
    }

    // Record initial output token balance
    let output_token_balance_before = ctx.accounts.pool_quote_token_account.amount;
    
    // Calculate minimum amount out for our sandwich buy
    // We use a more aggressive slippage for our transaction to ensure it goes through
    let minimum_out_for_sandwich = calculate_minimum_out_for_sandwich(
        optimal_buy_amount,
        total_input_amount,
        total_output_amount,
        global_config_data.coin_creator_fee_basis_points * 100u64,
        global_config_data.protocol_fee_basis_points * 100u64,
        global_config_data.lp_fee_basis_points * 100u64,
    )?;
    
    // Create the instruction data for the sell instruction
    let ix_data = PumpSwapSell {
        base_amount_in: optimal_buy_amount,
        min_quote_amount_out: minimum_out_for_sandwich,
    }
    .data();

    // Create the sell instruction for PumpSwap
    // First create the account metas for the required accounts
    let mut account_metas = vec![
        AccountMeta::new(pool.key(), false),
        AccountMeta::new(user.key(), true),
        AccountMeta::new_readonly(global_config.key(), false),
        AccountMeta::new_readonly(base_mint.key(), false),
        AccountMeta::new_readonly(quote_mint.key(), false),
        AccountMeta::new(user_base_token_account.key(), false),
        AccountMeta::new(user_quote_token_account.key(), false),
        AccountMeta::new(pool_base_token_account.key(), false),
        AccountMeta::new(pool_quote_token_account.key(), false),
        AccountMeta::new_readonly(protocol_fee_recipient.key(), false),
        AccountMeta::new(protocol_fee_recipient_token_account.key(), false),
        AccountMeta::new_readonly(base_token_program.key(), false),
        AccountMeta::new_readonly(quote_token_program.key(), false),
        AccountMeta::new_readonly(system_program.key(), false),
        AccountMeta::new_readonly(associated_token_program.key(), false),
        AccountMeta::new_readonly(event_authority.key(), false),
        AccountMeta::new_readonly(program.key(), false),
    ];

    // accounts for the instruction
    let mut accounts_vec = vec![
        pool,
        user,
        global_config,
        base_mint,
        quote_mint,
        user_base_token_account,
        user_quote_token_account,
        pool_base_token_account,
        pool_quote_token_account,
        protocol_fee_recipient,
        protocol_fee_recipient_token_account,
        base_token_program,
        quote_token_program,
        system_program,
        associated_token_program,
        event_authority,
        program,
    ];
    // Add coin creator accounts if provided
    if let Some(coin_creator_vault_ata) = &ctx.accounts.coin_creator_vault_ata {
        account_metas.push(AccountMeta::new(coin_creator_vault_ata.key(), false));
        accounts_vec.push(coin_creator_vault_ata.to_account_info());
    }

    if let Some(coin_creator_vault_authority) = &ctx.accounts.coin_creator_vault_authority {
        account_metas.push(AccountMeta::new_readonly(
            coin_creator_vault_authority.key(),
            false,
        ));
        accounts_vec.push(coin_creator_vault_authority.to_account_info());
    }
    let sell_ix = Instruction {
        program_id: pump_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // Invoke the PumpSwap sell instruction
    invoke_signed(&sell_ix, &accounts_vec, &[])?;

    // Calculate actual frontrun output amount
    let output_token_balance_after = ctx.accounts.user_quote_token_account.amount;
    let frontrun_output_amount =
        output_token_balance_after.saturating_sub(output_token_balance_before);

    // Store frontrun data in the PDA for the backrun to read
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = frontrun_output_amount;
    sandwich_state.frontrun_input_amount = optimal_buy_amount;
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_in_mint = *ctx.accounts.base_mint.to_account_info().key;
    sandwich_state.token_out_mint = *ctx.accounts.quote_mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;
        
    Ok(())
}

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

fn vault_amount_without_fee(
    vault_0: u64,
    vault_1: u64,
) -> (u64, u64) {
    (
        vault_0
            .checked_sub(Fees::protocol_fee(vault_0 as u128, 501).unwrap() as u64)
            .unwrap(),
        vault_1
            .checked_sub(Fees::protocol_fee(vault_1 as u128, 501).unwrap() as u64)
            .unwrap(),
    )
}

fn calculate_expected_output(
    amount_in: u64,
    reserve_in: u64,
    reserve_out: u64,
    trade_fee_rate: u64,
    protocol_fee_rate: u64,
    fund_fee_rate: u64,
) -> Result<u64> {
    // Use Raydium's CurveCalculator to calculate the expected output
    let result = CurveCalculator::swap_base_output(
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
