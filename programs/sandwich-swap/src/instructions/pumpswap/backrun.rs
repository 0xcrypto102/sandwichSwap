use anchor_lang::prelude::*;
use anchor_lang::solana_program::{instruction::Instruction, program::invoke_signed};

use crate::error::ErrorCode;
use crate::sandwich_state::SandwichCompleteEvent;
use super::{PumpSwapBuy, PumpSwapSell, PumpSwapContext};

/// Similar to swap_base_in, but used for completing the backrun part of a sandwich attack when the frontrun was a buy
pub fn pumpswap_backrun_buy(
    ctx: Context<PumpSwapContext>
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
    
    // Get the sandwich state to access frontrun data
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    
    // Verify this is the proper backrun for the frontrun that occurred
    if sandwich_state.is_complete {
        return err!(ErrorCode::SandwichAlreadyCompleted);
    }
    
    if sandwich_state.token_in_mint != ctx.accounts.base_mint.key() || 
       sandwich_state.token_out_mint != ctx.accounts.quote_mint.key() {
        return err!(ErrorCode::TokenMintMismatch);
    }
    
    // Prepare to sell the tokens we acquired in the frontrun
    let base_amount_in = sandwich_state.frontrun_output_amount;
    
    if base_amount_in == 0 {
        return err!(ErrorCode::EmptySupply);
    }
    
    // Record initial token balance to calculate profit later
    let quote_balance_before = ctx.accounts.user_quote_token_account.amount;
    
    // Create the instruction data for the sell instruction (since we're selling in the backrun)
    let ix_data = PumpSwapSell {
        base_amount_in,
        min_quote_amount_out: sandwich_state.frontrun_input_amount
            .checked_sub(sandwich_state.frontrun_input_amount
                .saturating_mul(90)
                .saturating_div(100)
            ).unwrap()
    }.data();

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

    // Create the instruction with all accounts
    let sell_ix = Instruction {
        program_id: pump_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // Invoke the PumpSwap sell instruction
    invoke_signed(&sell_ix, &accounts_vec, &[])?;

    // Calculate profit
    let quote_balance_after = ctx.accounts.user_quote_token_account.amount;
    let backrun_output_amount = quote_balance_after.saturating_sub(quote_balance_before);
    let profit = backrun_output_amount.saturating_sub(sandwich_state.frontrun_input_amount);

    // Update the sandwich state to complete
    sandwich_state.is_complete = true;

    // Emit sandwich complete event
    emit!(SandwichCompleteEvent {
        sandwich_id: sandwich_state.sandwich_id,
        profit,
        input_amount: sandwich_state.frontrun_input_amount,
        output_amount: backrun_output_amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}

/// Similar to swap_base_out, but used for completing the backrun part of a sandwich attack when the frontrun was a sell
pub fn pumpswap_backrun_sell(
    ctx: Context<PumpSwapContext>
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
    
    // Get the sandwich state to access frontrun data
    let sandwich_state = &mut ctx.accounts.sandwich_state;
    
    // Verify this is the proper backrun for the frontrun that occurred
    if sandwich_state.is_complete {
        return err!(ErrorCode::SandwichAlreadyCompleted);
    }
    
    if sandwich_state.token_out_mint != ctx.accounts.base_mint.key() || 
       sandwich_state.token_in_mint != ctx.accounts.quote_mint.key() {
        return err!(ErrorCode::TokenMintMismatch);
    }
    
    // In a backrun sell, we're buying back the base token by using the quote tokens we received
    let base_amount_out = sandwich_state.frontrun_output_amount;
    
    if base_amount_out == 0 {
        return err!(ErrorCode::EmptySupply);
    }

    // Record initial token balance to calculate profit later
    let base_balance_before = ctx.accounts.user_base_token_account.amount;
    
    // Create the instruction data for the buy instruction (since we're buying in the backrun)
    let ix_data = PumpSwapBuy {
        base_amount_out: sandwich_state.frontrun_input_amount,
        max_quote_amount_in: sandwich_state.frontrun_output_amount,
    }.data();

    // Create the buy instruction for PumpSwap
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

    // Create the instruction with all accounts
    let buy_ix = Instruction {
        program_id: pump_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    // Invoke the PumpSwap buy instruction
    invoke_signed(&buy_ix, &accounts_vec, &[])?;

    // Calculate profit
    let base_balance_after = ctx.accounts.user_base_token_account.amount;
    let backrun_output_amount = base_balance_after.saturating_sub(base_balance_before);
    
    // For sell backrun, the profit is calculated by comparing what we put in initially 
    // vs what we got back after the complete sandwich
    let profit = if backrun_output_amount <= sandwich_state.frontrun_input_amount {
        // If we spent less than our initial input, then it's pure profit
        sandwich_state.frontrun_input_amount.saturating_sub(backrun_output_amount)
    } else {
        // If we spent more, it's a loss
        0
    };

    // Update the sandwich state to complete
    sandwich_state.is_complete = true;
    
    // Emit sandwich complete event
    emit!(SandwichCompleteEvent {
        sandwich_id: sandwich_state.sandwich_id,
        profit,
        input_amount: sandwich_state.frontrun_input_amount,
        output_amount: backrun_output_amount,
        timestamp: Clock::get()?.unix_timestamp,
    });

    Ok(())
}