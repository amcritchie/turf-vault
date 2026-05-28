use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `cancel_contest` — refund the prize pool to the creator and flip
/// Contest.status to Cancelled.
///
/// Entry fees stay in operator-revenue (already collected, treated as
/// platform revenue per §11 Q1). Operator handles entrant compensation
/// off-chain via `mint_entry_token` goodwill credits.
///
/// Auth: 2-of-3 multisig.
///
/// Validations:
///   - multisig (constraint).
///   - status is Open or Locked (constraint via `ContestNotCancellable`).
///   - payout_mint pin (constraint).
///   - creator_token_account.authority == contest.creator (constraint).
///
/// Side effects:
///   - SPL Transfer (PDA-signed): prize_pool → creator_token_account
///     for the full on-chain prize_pool balance (defends against drift
///     vs the stored contest.prize_pool field).
///   - contest.status = Cancelled.
///
/// VaultState is zero-copy (v0.16) — load()? for bump + multisig.
#[derive(Accounts)]
pub struct CancelContest<'info> {
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
        constraint = (contest.status == ContestStatus::Open || contest.status == ContestStatus::Locked)
            @ VaultError::ContestNotCancellable,
    )]
    pub contest: Account<'info, Contest>,

    #[account(
        mut,
        seeds = [b"prize_pool", contest.contest_id.as_ref()],
        bump,
        token::mint = payout_mint,
        token::authority = vault_state,
    )]
    pub prize_pool: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = payout_mint.key() == vault_state.load()?.payout_mint @ VaultError::InvalidMint,
    )]
    pub payout_mint: Box<Account<'info, Mint>>,

    /// Creator's USDC ATA — destination of the prize pool refund.
    /// Constrained to belong to the contest's creator (via token::authority).
    #[account(
        mut,
        token::mint = payout_mint,
        token::authority = contest.creator,
    )]
    pub creator_token_account: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_cancel_contest(ctx: Context<CancelContest>) -> Result<()> {
    // Use the on-chain balance, not the stored field — defense against
    // dust or transfer-to-pool drift.
    let amount = ctx.accounts.prize_pool.amount;

    if amount > 0 {
        // Pull bump from a scoped load so we don't hold the Ref during CPI.
        let vault_bump = ctx.accounts.vault_state.load()?.bump;
        let vault_seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&vault_bump)];
        let signer_seeds = &[vault_seeds];

        let cpi_accounts = Transfer {
            from: ctx.accounts.prize_pool.to_account_info(),
            to: ctx.accounts.creator_token_account.to_account_info(),
            authority: ctx.accounts.vault_state.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, amount)?;
    }

    let contest = &mut ctx.accounts.contest;
    contest.status = ContestStatus::Cancelled;

    msg!(
        "Contest cancelled. prize_pool refunded: {} to creator: {}",
        amount,
        contest.creator
    );
    Ok(())
}
