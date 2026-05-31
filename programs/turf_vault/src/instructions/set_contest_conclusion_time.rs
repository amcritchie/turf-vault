use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `set_contest_conclusion_time` — set (or clear) a contest's conclusion
/// timestamp (v0.18). The conclusion marks when the contest is considered done:
/// once `Clock.unix_timestamp` passes it, `set_contest_lock_time` rejects — the
/// lock time is final. Derived like the lock (compared against the chain Clock,
/// no oracle). Mirrors set_contest_lock_time.
///
/// Auth: 1-of-3 vault signer. `new_conclusion_timestamp == 0` clears it
/// ("no conclusion scheduled"). Rejected once the contest has already concluded
/// (its current conclusion timestamp has passed) or been settled/cancelled.
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct SetContestConclusionTime<'info> {
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

pub fn handle_set_contest_conclusion_time(
    ctx: Context<SetContestConclusionTime>,
    new_conclusion_timestamp: i64,
) -> Result<()> {
    let contest = &mut ctx.accounts.contest;
    require!(
        contest.status != ContestStatus::Settled
            && contest.status != ContestStatus::Cancelled,
        VaultError::ContestAlreadySettled
    );
    // Once the contest has actually concluded, the conclusion is final too.
    if contest.conclusion_timestamp != 0 {
        require!(
            Clock::get()?.unix_timestamp < contest.conclusion_timestamp,
            VaultError::ContestConcluded
        );
    }
    contest.conclusion_timestamp = new_conclusion_timestamp;
    msg!(
        "Contest conclusion_timestamp set to {} for contest_id={:?}",
        new_conclusion_timestamp,
        contest.contest_id
    );
    Ok(())
}
