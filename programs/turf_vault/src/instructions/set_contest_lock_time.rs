use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `set_contest_lock_time` — set (or clear) a contest's derived lock timestamp.
///
/// Locking is a derived property (v0.17): `enter_contest{,_with_token}` reject
/// entries once `Clock.unix_timestamp >= contest.lock_timestamp`. This is the
/// ONLY lock mechanism — there is no separate manual lock/unlock instruction.
/// "Lock now" is expressed by passing the current chain time;
/// `new_lock_timestamp == 0` clears the lock (enterable indefinitely).
///
/// Auth: 1-of-3 vault signer. Adjusting the schedule is a routine op. The value
/// is unrestricted (may move earlier or later) EXCEPT once the contest has
/// concluded — there is no point re-opening a graded/cancelled contest.
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct SetContestLockTime<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

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
    let contest = &mut ctx.accounts.contest;
    // The lock time is final once the contest is settled/cancelled OR has
    // concluded (its conclusion_timestamp has passed) — see
    // set_contest_conclusion_time (v0.18).
    require!(
        contest.status != ContestStatus::Settled
            && contest.status != ContestStatus::Cancelled,
        VaultError::ContestAlreadySettled
    );
    if contest.conclusion_timestamp != 0 {
        require!(
            Clock::get()?.unix_timestamp < contest.conclusion_timestamp,
            VaultError::ContestConcluded
        );
    }
    contest.lock_timestamp = new_lock_timestamp;
    msg!(
        "Contest lock_timestamp set to {} for contest_id={:?}",
        new_lock_timestamp,
        contest.contest_id
    );
    Ok(())
}
