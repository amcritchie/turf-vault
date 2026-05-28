use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `lock_contest` — flip Contest.status from Open to Locked.
///
/// Blocks further entries (enter_contest{,_with_token} both check
/// `status == Open`). Settle and cancel remain available.
/// Useful for "lineups locked at kickoff" UX.
///
/// Auth: 1-of-3 vault signer. Locking is a routine op — going TO Locked
/// only restricts surface area. The reverse (unlock_contest) is 2-of-3
/// since it re-enables entries.
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct LockContest<'info> {
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
        constraint = contest.status == ContestStatus::Open @ VaultError::ContestNotOpen,
    )]
    pub contest: Account<'info, Contest>,
}

pub fn handle_lock_contest(ctx: Context<LockContest>) -> Result<()> {
    let contest = &mut ctx.accounts.contest;
    contest.status = ContestStatus::Locked;
    msg!("Contest locked: contest_id={:?}", contest.contest_id);
    Ok(())
}
