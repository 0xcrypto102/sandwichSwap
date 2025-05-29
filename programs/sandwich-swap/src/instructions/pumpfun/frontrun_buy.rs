use crate::error::ErrorCode;
use crate::instructions::pumpfun::bonding_curve::BondingCurveState;
use crate::instructions::pumpfun::{PumpFun, PUMPFUN_PROGRAM_ID};
use crate::sandwich_state::{SandwichState};
use anchor_lang::prelude::*;
use anchor_lang::prelude::{Account, Program, Signer};
use anchor_lang::solana_program::pubkey::Pubkey;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{Mint, Token, TokenAccount};
use solana_program::account_info::AccountInfo;
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed;

#[derive(Accounts)]
#[instruction(sandwich_id: String)]
pub struct PumpFunFrontrunBuyContext<'info> {
    /// CHECK: Global config
    pub global: AccountInfo<'info>,

    /// CHECK: Protocol fee recipient
    #[account(mut)]
    pub protocol_fee_recipient: AccountInfo<'info>,

    /// Base token mint (the token being bought or sold)
    pub mint: Box<Account<'info, Mint>>,

    /// CHECK: Bonding curve account
    #[account(mut)]
    pub bonding_curve: AccountLoader<'info, BondingCurveState>,

    /// Bonding curve token account
    #[account(mut)]
    pub bonding_curve_ata: Box<Account<'info, TokenAccount>>,

    /// User token account
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user
    )]
    pub user_ata: Account<'info, TokenAccount>,

    /// The user making the swap
    #[account(mut)]
    pub user: Signer<'info>,

    /// System program
    pub system_program: Program<'info, System>,

    /// token program
    pub token_program: Program<'info, Token>,

    /// CHECK: developer fee vault
    #[account(mut)]
    pub creator_fee_vault: AccountInfo<'info>,

    /// CHECK: Event authority account for PumpFun
    pub event_authority: AccountInfo<'info>,

    /// The pump fun program
    #[account(address = PUMPFUN_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub pump_program: Program<'info, PumpFun>,

    /// The account that will store sandwich state
    #[account(
       init_if_needed,
       payer = user,
       space = 8 + SandwichState::SIZE,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump
    )]
    pub sandwich_state: Account<'info, SandwichState>,

    // Associated token program for init_if_needed
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize)]
pub struct PumpFunBuy {
    pub token_amount: u64,
    pub max_sol_cost: u64,
}

impl PumpFunBuy {
    pub fn data(&self) -> Vec<u8> {
        let mut data = vec![250, 234, 13, 123, 213, 156, 19, 236]; // buy instruction discriminator
        data.extend_from_slice(&self.token_amount.to_le_bytes());
        data.extend_from_slice(&self.max_sol_cost.to_le_bytes());
        data
    }
}

/// Computes safe front‑run parameters **with a 1 % fee** on every swap
/// and verifies that the sandwich profit ≥ min_profit_pct (0.5 % = 0.005).
///
/// Returns:
///   Some((my_token_amount_out, my_max_sol_amount_in, profit_pct))
///   or None if slippage would be violated OR profit is below the floor.
///
///  – All math is f64 for clarity.  Use fixed‑point u128 in production. –
///
///  Curve: constant‑product k = x·y                 (no time‑varying k)
///  Fee:   taken on swap‑input, i.e.  Δ_in_eff = Δ_in * (1‑fee)
///
fn compute_front_run_with_fee(
    v_tokens: u64,
    v_sol: u64,
    target_token_amount_out: u64,
    target_max_sol_amount_in: u64,
    fee: f64,             // e.g. 0.01 for 1 %
    min_profit_pct: f64,  // e.g. 0.005 for 0.5 %
) -> Option<(u64, u64, f64)> {
    let g = 1.0 - fee;               // 0.99
    let x0 = v_tokens as f64;        // initial virtual token reserve
    let y0 = v_sol    as f64;        // initial virtual SOL reserve
    let t  = target_token_amount_out  as f64; // victim’s token buy size  (T)
    let m  = target_max_sol_amount_in as f64; // victim’s SOL slippage cap (M)
    let k  = x0 * y0;                // invariant

    // ---------- 1. max‑allowed SOL front‑run (Δ) ----------
    //
    // Quadratic in Y = y0 + Δ*g :
    //     T·Y² + (M·g·T)·Y − (M·g·k) = 0
    // Pick the positive root, then Δ = (Y − y0)/g
    //
    let disc  = m * g * t * (m * g * t + 4.0 * k); // discriminant
    let sqrt  = disc.sqrt();
    let y_max = (-m * g * t + sqrt) / (2.0 * t);

    if y_max <= y0 {
        return None;                  // no room → any sandwich breaks slippage
    }
    let delta_sol = (y_max - y0) / g; // total SOL you may send (before fee)

    if delta_sol <= 0.0 {
        return None;
    }

    // ---------- 2. your front‑run token out ----------
    //
    // token_out = x0 − k / y_max
    //
    let token_out_me = x0 - k / y_max;
    if token_out_me <= 0.0 {
        return None;
    }

    // ---------- 3. simulate victim buy ----------
    //
    // After *your* buy the pool is at (x1 = k / y_max , y_max)
    // Victim buys T tokens, paying S SOL (guaranteed ≤ M by construction).
    //
    let x1 = k / y_max;
    let x2 = x1 - t;              // pool tokens after victim
    if x2 <= 0.0 {
        return None;              // victim would empty pool (shouldn’t happen)
    }
    let y2 = k / x2;              // pool SOL after victim

    // ---------- 4. simulate your back‑run sell ----------
    //
    // You return token_out_me tokens.  Input fee is applied again.
    //
    let x3 = x2 + token_out_me * g;
    let y3 = k / x3;

    let revenue_sol = y2 - y3;                 // SOL you take out
    let profit      = revenue_sol - delta_sol; // net after paying Δ on buy
    let profit_pct  = profit / delta_sol;

    if profit_pct < min_profit_pct {
        return None;            // not profitable enough
    }

    // ---------- 5. final, quantised values ----------
    let my_max_sol_in      = delta_sol.floor()   as u64;
    let my_token_amount_out = token_out_me.floor() as u64;

    Some((my_token_amount_out, my_max_sol_in, profit_pct))
}

pub fn pumpfun_frontrun_buy(
    ctx: Context<PumpFunFrontrunBuyContext>,
    target_token_amount_out: u64,
    target_max_sol_amount_in: u64,
    sandwich_id: u64,
) -> Result<()> {
    let curve_state = &mut ctx.accounts.bonding_curve.load_mut()?;
    let v_tokens = curve_state.virtual_token_reserves;
    let v_sol    = curve_state.virtual_sol_reserves;
    let price_now = v_sol as f64 / v_tokens as f64;
    let cost_now  = target_token_amount_out as f64 * price_now;
    require!(
        cost_now < target_max_sol_amount_in as f64,
        ErrorCode::ExceededSlippage
    );

    const FEE: f64 = 0.01; // 1%
    const MIN_PROFIT: f64 = 0.005; // 0.5%

    let (frontrun_token_out, frontrun_max_sol_in, _profit_pct) = compute_front_run_with_fee(
        v_tokens,
        v_sol,
        target_token_amount_out,
        target_max_sol_amount_in,
        FEE,
        MIN_PROFIT,
    ).ok_or(ErrorCode::UnprofitableSandwich)?;

    let account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.global.key(), false),
        AccountMeta::new(ctx.accounts.protocol_fee_recipient.key(), false),
        AccountMeta::new_readonly(ctx.accounts.mint.key(), false),
        AccountMeta::new(ctx.accounts.bonding_curve.key(), false),
        AccountMeta::new(ctx.accounts.bonding_curve_ata.key(), false),
        AccountMeta::new(ctx.accounts.user_ata.key(), false),
        AccountMeta::new(ctx.accounts.user.key(), true),
        AccountMeta::new_readonly(ctx.accounts.system_program.key(), false),
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new(ctx.accounts.creator_fee_vault.key(), false),
        AccountMeta::new_readonly(ctx.accounts.event_authority.key(), false),
        AccountMeta::new_readonly(ctx.accounts.pump_program.key(), false)
    ];

    let accounts_vec = vec![
        ctx.accounts.global.to_account_info(),
        ctx.accounts.protocol_fee_recipient.to_account_info(),
        ctx.accounts.mint.to_account_info(),
        ctx.accounts.bonding_curve.to_account_info(),
        ctx.accounts.bonding_curve_ata.to_account_info(),
        ctx.accounts.user_ata.to_account_info(),
        ctx.accounts.user.to_account_info(),
        ctx.accounts.system_program.to_account_info(),
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.creator_fee_vault.to_account_info(),
        ctx.accounts.event_authority.to_account_info(),
        ctx.accounts.pump_program.to_account_info(),
    ];

    let ix_data = PumpFunBuy {
        token_amount: frontrun_token_out,
        max_sol_cost: frontrun_max_sol_in,
    }.data();

    let buy_ix = Instruction {
        program_id: ctx.accounts.pump_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let lamports_before = ctx.accounts.user.lamports();
    invoke_signed(&buy_ix, &accounts_vec, &[])?;
    let lamports_after = ctx.accounts.user.lamports();

    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = frontrun_token_out;
    sandwich_state.frontrun_input_amount = lamports_after.saturating_sub(lamports_before);
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_out_mint = *ctx.accounts.mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;

    Ok(())
}
