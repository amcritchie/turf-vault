use anchor_lang::prelude::*;
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `close_contest` — reclaim SOL rent from a settled Contest PDA.
///
/// Anchor's `close = admin` directive transfers the account's lamports
/// (the rent we paid at create time) to `admin` and zeros the data.
/// The contest is effectively garbage-collected.
///
/// Refuses to run on un-settled contests — settle_contest must complete
/// first (status must be Settled).
///
/// Auth: 1-of-3 vault signer.
#[derive(Accounts)]
pub struct CloseContest<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        constraint = contest.status == ContestStatus::Settled @ VaultError::ContestNotSettled,
        close = admin,
    )]
    pub contest: Account<'info, Contest>,
}

pub fn handle_close_contest(_ctx: Context<CloseContest>) -> Result<()> {
    msg!("Contest account closed, rent reclaimed");
    Ok(())
}
