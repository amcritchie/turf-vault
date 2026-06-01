use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `set_contest_conclusion_time` — set (or clear) a contest's conclusion
/// timestamp (v0.18). The conclusion marks when the contest is considered done:
/// once `Clock.unix_timestamp` passes it, `set_contest_lock_time` rejects — the
/// lock time is final. Derived like the lock (compared against the chain Clock,
/// no oracle).
///
/// Auth (v0.19, audit #5):
///   - Setting it for the FIRST time (current conclusion == 0): 1-of-3.
///   - AMENDING an already-set conclusion (the finality marker): 2-of-3
///     (admin + distinct `cosigner`). Clearing/postponing it at 1-of-3 would
///     let a single key re-arm the relock — the same late-entry vector as #5.
///   - Always rejected once the contest is settled/cancelled or has concluded.
///
/// Timestamps must be non-negative; a set conclusion must follow a set lock.
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct SetContestConclusionTime<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    /// Optional second vault signer. Required (2-of-3) ONLY to amend an
    /// already-set conclusion; omitted for the initial 1-of-3 set.
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

pub fn handle_set_contest_conclusion_time(
    ctx: Context<SetContestConclusionTime>,
    new_conclusion_timestamp: i64,
) -> Result<()> {
    let now = Clock::get()?.unix_timestamp;

    require!(
        ctx.accounts.contest.status != ContestStatus::Settled
            && ctx.accounts.contest.status != ContestStatus::Cancelled,
        VaultError::ContestAlreadySettled
    );

    // Once the contest has actually concluded, the conclusion is final.
    let current_conclusion = ctx.accounts.contest.conclusion_timestamp;
    if current_conclusion != 0 {
        require!(now < current_conclusion, VaultError::ContestConcluded);
    }

    // Timestamp validity (audit #5): non-negative; and if set, must be in the
    // FUTURE and follow a set lock. The future check matters even on a 1-of-3
    // first set: a past conclusion would immediately conclude the contest —
    // enabling settle (the #6 gate) and bricking set_contest_lock_time
    // (ContestConcluded) — a single-key bypass of the finality guarantee.
    require!(new_conclusion_timestamp >= 0, VaultError::InvalidTimestamp);
    if new_conclusion_timestamp != 0 {
        require!(new_conclusion_timestamp > now, VaultError::InvalidTimestamp);
        let lock = ctx.accounts.contest.lock_timestamp;
        if lock != 0 {
            require!(new_conclusion_timestamp > lock, VaultError::InvalidTimestamp);
        }
    }

    // Audit #5: amending an already-set conclusion (clear/postpone) requires
    // 2-of-3 so a single key can't defeat the "lock is final once concluded"
    // guarantee. Setting it the first time (0 -> value) stays 1-of-3.
    if current_conclusion != 0 {
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

    ctx.accounts.contest.conclusion_timestamp = new_conclusion_timestamp;
    msg!(
        "Contest conclusion_timestamp set to {} for contest_id={:?} (2-of-3 amend: {})",
        new_conclusion_timestamp,
        ctx.accounts.contest.contest_id,
        current_conclusion != 0
    );
    Ok(())
}
