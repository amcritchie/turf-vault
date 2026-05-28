use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `unlock_contest` — safety hatch for lock_contest. Flips Contest.status
/// from Locked back to Open.
///
/// Use case: a contest was locked too early, a game was postponed, etc.
///
/// Auth: 2-of-3 multisig — higher than `lock_contest` because returning
/// to Open re-enables entries (a one-way restriction-relaxer is treasury-
/// level, not routine).
///
/// VaultState is zero-copy (v0.16) — read via `load()?`.
#[derive(Accounts)]
pub struct UnlockContest<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"contest", contest.contest_id.as_ref()],
        bump = contest.bump,
        constraint = contest.status == ContestStatus::Locked @ VaultError::ContestNotLocked,
    )]
    pub contest: Account<'info, Contest>,
}

pub fn handle_unlock_contest(ctx: Context<UnlockContest>) -> Result<()> {
    let contest = &mut ctx.accounts.contest;
    contest.status = ContestStatus::Open;
    msg!("Contest unlocked: contest_id={:?}", contest.contest_id);
    Ok(())
}
