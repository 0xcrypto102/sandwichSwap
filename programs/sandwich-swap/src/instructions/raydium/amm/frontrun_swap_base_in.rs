use crate::error::ErrorCode;
use crate::instructions::{Amm, AmmAuthority, Serum, AMM_AUTHORITY_ID, SERUM_PROGRAM_ID, AMM_PROGRAM_ID, Swap};
use crate::sandwich_state::SandwichState;
use anchor_lang::prelude::*;
use anchor_spl::associated_token::AssociatedToken;
use anchor_spl::token::{spl_token, Mint, Token, TokenAccount};
use solana_program::instruction::Instruction;
use solana_program::program::invoke_signed;
use crate::instructions::amm::pair::ProgramAccount;

#[derive(Accounts, Clone)]
#[instruction(sandwich_id: String)]
pub struct AmmFrontrunSwapBaseIn<'info> {
    /// token program
    pub token_program: Program<'info, Token>,

    /// CHECK Pair account
    #[account(mut)]
    pub amm: AccountLoader<'info, ProgramAccount>,

    /// Raydium Authority
    #[account(address = AMM_AUTHORITY_ID.parse::<Pubkey>().unwrap())]
    pub amm_authority: Program<'info, AmmAuthority>,

    /// CHECK Open Orders account
    #[account(mut)]
    pub amm_open_orders: AccountInfo<'info>,

    /// CHECK Target Orders account
    #[account(mut)]
    pub amm_target_orders: AccountInfo<'info>,

    /// Pool base token account
    #[account(mut)]
    pub pool_coin_token_account: Box<Account<'info, TokenAccount>>,

    /// Pool quote token account
    #[account(mut)]
    pub pool_pc_token_account: Box<Account<'info, TokenAccount>>,

    /// OpenBook program id
    #[account(address = SERUM_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub serum_program: Program<'info, Serum>,

    /// CHECK Serum market account
    #[account(mut)]
    pub serum_market: AccountInfo<'info>,

    /// CHECK Serum bids account
    #[account(mut)]
    pub serum_bids: AccountInfo<'info>,

    /// CHECK Serum asks account
    #[account(mut)]
    pub serum_asks: AccountInfo<'info>,

    /// CHECK Serum event queue account
    #[account(mut)]
    pub serum_event_queue: AccountInfo<'info>,

    /// Pool base token account
    #[account(mut)]
    pub serum_coin_vault_account: Box<Account<'info, TokenAccount>>,

    /// Pool quote token account
    #[account(mut)]
    pub serum_pc_vault_account: Box<Account<'info, TokenAccount>>,

    /// CHECK Serum vault signer account
    pub serum_vault_signer: AccountInfo<'info>,

    /// User source token account
    #[account(mut)]
    pub user_source_token_account: Box<Account<'info, TokenAccount>>,

    /// User destination token account
    #[account(
        init_if_needed,
        payer = user_source_owner,
        associated_token::mint = base_mint,
        associated_token::authority = user_source_owner
    )]
    pub user_target_token_account: Box<Account<'info, TokenAccount>>,

    /// The user making the swap
    #[account(mut)]
    pub user_source_owner: Signer<'info>,

    /// The account that will store sandwich state
    #[account(
       init_if_needed,
       payer = user_source_owner,
       space = 8 + SandwichState::SIZE,
       seeds = [b"sandwich", sandwich_id.as_bytes()],
       bump
    )]
    pub sandwich_state: Account<'info, SandwichState>,

    // Associated token program for init_if_needed
    pub associated_token_program: Program<'info, AssociatedToken>,

    /// System program
    pub system_program: Program<'info, System>,

    /// AMM Program
    #[account(address = AMM_PROGRAM_ID.parse::<Pubkey>().unwrap())]
    pub amm_program: Program<'info, Amm>,

    /// base mint
    #[account(
        constraint = base_mint.key() == amm.load()?.base_mint
    )]
    pub base_mint: Account<'info, Mint>,
}

/// Computes the maximum base‑in amount you can swap **before** the victim
/// so that their `minimum_amount_out` is still satisfied, **including**
/// Raydium’s input fee (default tier: 0.25 % of which 16 % is kept).
///
/// Returns:
///   • my_amount_in        – base/coin lamports you should swap
///   • my_min_amount_out   – quote/pc lamports you expect (with 0.2 % slack)
///   • profit_pct          – sandwich profit in base, relative to amount‑in
///
/// Returns `None` if the sandwich would break slippage **or** profit < floor.
fn compute_front_run_base_in_with_fee(
    x_base_reserve: u64,          // pool coin reserve      (x₀)
    y_quote_reserve: u64,         // pool pc   reserve      (y₀)
    target_amount_in: u64,        // victim amount_in       (Δₜ raw)
    target_min_amount_out: u64,   // victim minimum_out     (M)
    fee_fraction: f64,            // 0.0004  (Raydium v4 default)
    min_profit_pct: f64,          // 0.005   (0.5 %)
) -> Option<(u64 /*my_amount_in*/,
             u64 /*my_min_amount_out*/,
             f64 /*profit_pct*/)> {

    // ---------- constants ----------
    let g = 1.0 - fee_fraction;                 // fraction that reaches pool
    let x0 = x_base_reserve  as f64;
    let y0 = y_quote_reserve as f64;
    let k  = x0 * y0;                           // invariant

    let dt_eff = target_amount_in as f64 * g;   // Δₜ·g   (effective add to x)
    let m      = target_min_amount_out as f64;  // M

    // ---------- quadratic coeffs  (see derivation in the answer) ----------
    let a = m;                                  // a = M
    let b = m * (dt_eff + 2.0 * x0);            // b = M (Δₜ·g + 2x₀)
    let c = m * dt_eff * x0 + m * x0 * x0       // c = M (Δₜ·g·x₀ + x₀²)
        - g * dt_eff * x0 * y0;             //     − gΔₜ x₀ y₀

    let disc = b * b - 4.0 * a * c;
    if disc <= 0.0 { return None; }             // victim already fails

    let d_max = (-b + disc.sqrt()) / (2.0 * a); // D = g · my_amount_in
    if d_max <= 0.0 { return None; }

    let my_amount_in = (d_max / g).floor() as u64;
    if my_amount_in == 0 { return None; }

    // ---------- our front‑run quote out ----------
    let y1     = k / (x0 + d_max);
    let q_out  = y0 - y1;                       // quote we receive
    if q_out <= 0.0 { return None; }

    // ---------- simulate victim then our back‑run (quote‑in) ----------
    let x1         = x0 + d_max;
    let x2         = x1 + dt_eff;
    let y2         = k / x2;
    let q_eff_back = q_out * g;                 // quote reaches pool (fee again)
    let y3         = y2 + q_eff_back;
    let x3         = k / y3;
    let base_back  = x2 - x3;                   // we receive in back‑run
    let profit     = base_back - (d_max / g);   // net in base/coin
    let profit_pct = profit / (d_max / g);

    if profit_pct < min_profit_pct { return None; }

    // Provide a 0.2 % personal slippage cushion on our min_out
    let my_min_amount_out = (q_out * 0.998).floor() as u64;

    Some((my_amount_in, my_min_amount_out, profit_pct))
}

/// swap_base_in instruction
pub fn amm_frontrun_swap_base_in(
    ctx: Context<AmmFrontrunSwapBaseIn>,
    target_amount_in: u64,
    target_minimum_amount_out: u64,
    sandwich_id: u64,
) -> Result<()> {
    let pool_coin  = ctx.accounts.pool_coin_token_account.amount;
    let pool_quote = ctx.accounts.pool_pc_token_account.amount;

    let amm_state = ctx.accounts.amm.load()?;

    let trade_fee = amm_state.trade_fee_numerator as f64
        / amm_state.trade_fee_denominator as f64;
    let swap_fee  = amm_state.swap_fee_numerator  as f64
        / amm_state.swap_fee_denominator  as f64;
    let fee_fraction = swap_fee + trade_fee * 0.16;

    const MIN_PROFIT: f64 = 0.005; // 0.5%

    let (frontrun_amount_in, frontrun_min_out, _profit_pct) =
        compute_front_run_base_in_with_fee(
            pool_coin,
            pool_quote,
            target_amount_in,
            target_minimum_amount_out,
            fee_fraction,
            MIN_PROFIT,
        ).ok_or(ErrorCode::UnprofitableSandwich)?;

    let account_metas = vec![
        AccountMeta::new_readonly(ctx.accounts.token_program.key(), false),
        AccountMeta::new(ctx.accounts.amm.key(), false),
        AccountMeta::new_readonly(ctx.accounts.amm_authority.key(), false),
        AccountMeta::new(ctx.accounts.amm_open_orders.key(), false),
        AccountMeta::new(ctx.accounts.amm_target_orders.key(), false),
        AccountMeta::new(ctx.accounts.pool_coin_token_account.key(), false),
        AccountMeta::new(ctx.accounts.pool_pc_token_account.key(), false),
        AccountMeta::new_readonly(ctx.accounts.serum_program.key(), false),
        AccountMeta::new(ctx.accounts.serum_market.key(), false),
        AccountMeta::new(ctx.accounts.serum_bids.key(), false),
        AccountMeta::new(ctx.accounts.serum_asks.key(), false),
        AccountMeta::new(ctx.accounts.serum_event_queue.key(), false),
        AccountMeta::new(ctx.accounts.serum_coin_vault_account.key(), false),
        AccountMeta::new(ctx.accounts.serum_pc_vault_account.key(), false),
        AccountMeta::new_readonly(ctx.accounts.serum_vault_signer.key(), false),
        AccountMeta::new(ctx.accounts.user_source_token_account.key(), false),
        AccountMeta::new(ctx.accounts.user_target_token_account.key(), false),
        AccountMeta::new(ctx.accounts.user_source_owner.key(), true),
    ];

    let accounts_vec = vec![
        ctx.accounts.token_program.to_account_info(),
        ctx.accounts.amm.to_account_info(),
        ctx.accounts.amm_authority.to_account_info(),
        ctx.accounts.amm_open_orders.to_account_info(),
        ctx.accounts.amm_target_orders.to_account_info(),
        ctx.accounts.pool_coin_token_account.to_account_info(),
        ctx.accounts.pool_pc_token_account.to_account_info(),
        ctx.accounts.serum_program.to_account_info(),
        ctx.accounts.serum_market.to_account_info(),
        ctx.accounts.serum_bids.to_account_info(),
        ctx.accounts.serum_asks.to_account_info(),
        ctx.accounts.serum_event_queue.to_account_info(),
        ctx.accounts.serum_coin_vault_account.to_account_info(),
        ctx.accounts.serum_pc_vault_account.to_account_info(),
        ctx.accounts.serum_vault_signer.to_account_info(),
        ctx.accounts.user_source_token_account.to_account_info(),
        ctx.accounts.user_target_token_account.to_account_info(),
        ctx.accounts.user_source_owner.to_account_info(),
    ];

    let ix_data = Swap {
        discriminator: 9,
        amount_in: frontrun_amount_in,
        min_amount_out: frontrun_min_out,
    }.data();

    let buy_ix = Instruction {
        program_id: ctx.accounts.amm_program.key(),
        accounts: account_metas,
        data: ix_data,
    };

    let lamports_before = ctx.accounts.user_source_token_account.amount;
    invoke_signed(&buy_ix, &accounts_vec, &[])?;

    ctx.accounts.user_source_token_account.reload()?;
    ctx.accounts.user_target_token_account.reload()?;
    let lamports_after = ctx.accounts.user_source_token_account.amount;

    let sandwich_state = &mut ctx.accounts.sandwich_state;
    sandwich_state.frontrun_output_amount = ctx.accounts.user_target_token_account.amount;
    sandwich_state.frontrun_input_amount = lamports_after.saturating_sub(lamports_before);
    sandwich_state.sandwich_id = sandwich_id;
    sandwich_state.token_in_mint = spl_token::native_mint::id();
    sandwich_state.token_out_mint = *ctx.accounts.base_mint.to_account_info().key;
    sandwich_state.timestamp = Clock::get()?.unix_timestamp;
    sandwich_state.is_complete = false;
    sandwich_state.bump = ctx.bumps.sandwich_state;

    Ok(())
}
