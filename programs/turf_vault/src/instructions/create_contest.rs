use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, Contest, ContestStatus, MAX_CURRENCIES};
use crate::errors::VaultError;

/// `create_contest` — set up a new contest with a per-currency fee schedule
/// and a USDC prize pool.
///
/// Dual-signer:
///   - `payer` (any 1-of-3 vault signer): pays SOL rent for the Contest PDA
///     and the per-contest prize_pool ATA.
///   - `creator` (any wallet with USDC): signs the SPL transfer of
///     `prize_pool` USDC from their ATA → the per-contest [b"prize_pool",
///     contest_id] PDA (authority = vault_state).
///
/// For operator-funded contests, admin acts as BOTH payer and creator (Solana
/// dedupes signers by pubkey so a single signature covers both slots).
///
/// Validations:
///   1. `payer` is a vault signer (constraint above).
///   2. checked sum of `payout_amounts == prize_pool`.
///   3. `payout_amounts.len() <= 10` (Anchor `max_len`).
///   4. For each `i < MAX_CURRENCIES` where `entry_fee_by_currency[i] > 0`:
///      slot must be registered AND active.
///   5. At least one `entry_fee_by_currency[i] > 0` OR `prize_pool > 0`
///      (Q6: zero-fee zero-prize contests rejected).
///
/// `season_id` binds the contest to one Season for seed-schedule pinning
/// (OPSEC-023).
///
/// VaultState is zero-copy (v0.16) — read via `load()?` for the
/// per-currency validation.
#[derive(Accounts)]
#[instruction(contest_id: [u8; 32])]
pub struct CreateContest<'info> {
    /// 1-of-3 vault signer — pays SOL rent for Contest PDA + prize_pool PDA.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Creator's wallet — signs USDC transfer for the prize pool.
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&payer.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    #[account(
        init,
        payer = payer,
        space = 8 + Contest::INIT_SPACE,
        seeds = [b"contest", contest_id.as_ref()],
        bump,
    )]
    pub contest: Box<Account<'info, Contest>>,

    /// Per-contest USDC prize pool. PDA at [b"prize_pool", contest_id],
    /// authority = vault_state. Drained on settle_contest, cancel_contest,
    /// or close_contest (dust sweep).
    #[account(
        init,
        payer = payer,
        token::mint = payout_mint,
        token::authority = vault_state,
        seeds = [b"prize_pool", contest_id.as_ref()],
        bump,
    )]
    pub prize_pool: Box<Account<'info, TokenAccount>>,

    /// Must equal vault_state.payout_mint (canonical USDC).
    #[account(
        constraint = payout_mint.key() == vault_state.load()?.payout_mint @ VaultError::InvalidMint,
    )]
    pub payout_mint: Box<Account<'info, Mint>>,

    /// Creator's USDC ATA — source of the prize pool.
    #[account(
        mut,
        token::mint = payout_mint,
        token::authority = creator,
    )]
    pub creator_token_account: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_create_contest(
    ctx: Context<CreateContest>,
    contest_id: [u8; 32],
    season_id: u32,
    entry_fee_by_currency: [u64; MAX_CURRENCIES],
    max_entries: u32,
    payout_amounts: Vec<u64>,
    prize_pool: u64,
    lock_timestamp: i64,
) -> Result<()> {
    // Validation #2: payout sum == prize_pool (checked add — OPSEC-025).
    let total_payouts: u64 = payout_amounts
        .iter()
        .try_fold(0u64, |acc, &x| acc.checked_add(x))
        .ok_or(VaultError::Overflow)?;
    require!(total_payouts == prize_pool, VaultError::InvalidPayoutTiers);

    // Validation #4: for each currency slot with a fee, slot must be
    // registered AND active. Scope the vault Ref tight.
    let mut any_fee_set = false;
    {
        let vault = ctx.accounts.vault_state.load()?;
        for i in 0..MAX_CURRENCIES {
            if entry_fee_by_currency[i] > 0 {
                any_fee_set = true;
                require!(
                    vault.accepted_currencies[i].mint != Pubkey::default(),
                    VaultError::InvalidCurrencyIndex
                );
                require!(
                    vault.accepted_currencies[i].active == 1,
                    VaultError::CurrencyNotActive
                );
            }
        }
    }

    // Validation #5: at least one fee OR a non-zero prize_pool.
    require!(
        any_fee_set || prize_pool > 0,
        VaultError::FeeAndPrizeBothZero
    );

    let contest = &mut ctx.accounts.contest;
    contest.contest_id = contest_id;
    contest.admin = ctx.accounts.payer.key();
    contest.creator = ctx.accounts.creator.key();
    contest.season_id = season_id;
    contest.prize_pool = prize_pool;
    contest.entry_fee_by_currency = entry_fee_by_currency;
    contest.entry_fees = [0u64; MAX_CURRENCIES];
    contest.max_entries = max_entries;
    contest.current_entries = 0;
    contest.status = ContestStatus::Open;
    contest.payout_amounts = payout_amounts;
    contest.bump = ctx.bumps.contest;
    // 0 = no lock scheduled; a non-zero value gates entries once chain time
    // passes it (see enter_contest / enter_contest_with_token).
    contest.lock_timestamp = lock_timestamp;
    // Conclusion is set later via set_contest_conclusion_time (0 = none yet).
    contest.conclusion_timestamp = 0;
    contest._reserved = [0; 16];

    // Fund the prize pool via SPL transfer from the creator's ATA.
    if prize_pool > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.creator_token_account.to_account_info(),
            to: ctx.accounts.prize_pool.to_account_info(),
            authority: ctx.accounts.creator.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, prize_pool)?;
    }

    msg!(
        "Contest created. prize_pool: {}, max_entries: {}, season: {}",
        prize_pool,
        max_entries,
        season_id,
    );
    Ok(())
}
