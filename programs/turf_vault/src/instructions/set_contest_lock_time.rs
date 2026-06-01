use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `set_contest_lock_time` — set (or clear) a contest's derived lock timestamp.
///
/// Locking is a derived property (v0.17): `enter_contest{,_with_token}` reject
/// entries once `Clock.unix_timestamp >= contest.lock_timestamp`. This is the
/// ONLY lock mechanism. "Lock now" is expressed by passing the current chain
/// time; `new_lock_timestamp == 0` clears the lock (enterable indefinitely).
///
/// Auth (v0.19, audit #5):
///   - BEFORE the lock has passed: 1-of-3 vault signer (routine reschedule).
///   - AFTER the lock has passed (lock_timestamp != 0 && now >= it): amending
///     it re-opens a contest whose entry window already closed — the
///     results-known late-entry vector — so it requires 2-of-3 (admin + a
///     distinct `cosigner`). Pass `cosigner` only for that case.
///   - Always rejected once the contest is settled/cancelled or has concluded.
///
/// Timestamps must be non-negative; a set lock must precede a set conclusion.
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct SetContestLockTime<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Optional second vault signer. Required (2-of-3) ONLY to amend a lock
    /// that has already passed; omitted for a routine pre-lock set (1-of-3).
    pub cosigner: Option<Signer<'info>>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"contest", contest.contest_id.as_ref()],
        bump = contest.bump,
    )]
    pub contest: Account<'info, Contest>,
}

pub fn handle_set_contest_lock_time(
    ctx: Context<SetContestLockTime>,
    new_lock_timestamp: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    // Final once settled/cancelled or concluded.
    require!(
        ctx.accounts.contest.status != ContestStatus::Settled
            && ctx.accounts.contest.status != ContestStatus::Cancelled,
        VaultError::ContestAlreadySettled
    );
    let conclusion = ctx.accounts.contest.conclusion_timestamp;
    if conclusion != 0 {
        require!(now < conclusion, VaultError::ContestConcluded);
    }

    // Timestamp validity (audit #5): non-negative; a set lock must precede a
    // set conclusion. (A past/now value is allowed — it locks immediately.)
    require!(new_lock_timestamp >= 0, VaultError::InvalidTimestamp);
    if new_lock_timestamp != 0 && conclusion != 0 {
        require!(new_lock_timestamp < conclusion, VaultError::InvalidTimestamp);
    }

    // Audit #5 (re-open protection): once the lock has PASSED, amending it
    // re-opens a closed entry window — require 2-of-3 (admin + distinct cosigner).
    let current_lock = ctx.accounts.contest.lock_timestamp;
    let lock_engaged = current_lock != 0 && now >= current_lock;
    if lock_engaged {
        let vault_state = ctx.accounts.vault_state.load()?;
        let cosigner = ctx
            .accounts
            .cosigner
            .as_ref()
            .ok_or(VaultError::Unauthorized)?;
        require!(
            vault_state.validate_multisig(&ctx.accounts.admin.key(), &cosigner.key()),
            VaultError::Unauthorized
        );
    }

    ctx.accounts.contest.lock_timestamp = new_lock_timestamp;
    msg!(
        "Contest lock_timestamp set to {} for contest_id={:?} (post-lock 2-of-3 amend: {})",
        new_lock_timestamp,
        ctx.accounts.contest.contest_id,
        lock_engaged
    );
    Ok(())
}
