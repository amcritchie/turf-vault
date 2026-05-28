use anchor_lang::prelude::*;
use anchor_spl::token::{self, CloseAccount, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `close_contest` — reclaim SOL rent from a settled (or cancelled) Contest.
///
/// Closes:
///   1. The prize_pool ATA, after sweeping any residual USDC to the
///      operator-revenue USDC ATA (§11 Q8 — handles 1-USDC-dust from
///      rounding errors without blocking the close).
///   2. The Contest PDA itself, transferring its lamports to `admin`.
///
/// Refuses to run on un-finalized contests — status must be Settled or
/// Cancelled.
///
/// Auth: 1-of-3 vault signer.
///
/// VaultState is zero-copy (v0.16) — load()? for bump + signer check.
#[derive(Accounts)]
pub struct CloseContest<'info> {
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
        constraint = (contest.status == ContestStatus::Settled || contest.status == ContestStatus::Cancelled)
            @ VaultError::ContestNotSettled,
        close = admin,
    )]
    pub contest: Account<'info, Contest>,

    /// Prize pool ATA — closed in this instruction. Any residual balance
    /// is swept to op_rev_usdc_ata first (decided §11 Q8).
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

    /// Operator-revenue USDC ATA — destination for any residual prize_pool
    /// USDC. PDA-bound to the payout mint at [b"op_rev", payout_mint].
    #[account(
        mut,
        seeds = [b"op_rev", payout_mint.key().as_ref()],
        bump,
        token::mint = payout_mint,
        token::authority = vault_state,
    )]
    pub op_rev_usdc_ata: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_close_contest(ctx: Context<CloseContest>) -> Result<()> {
    let dust = ctx.accounts.prize_pool.amount;

    let vault_bump = ctx.accounts.vault_state.load()?.bump;
    let vault_seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&vault_bump)];
    let signer_seeds = &[vault_seeds];

    // Sweep any residual prize_pool USDC → op_rev_usdc_ata.
    if dust > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.prize_pool.to_account_info(),
            to: ctx.accounts.op_rev_usdc_ata.to_account_info(),
            authority: ctx.accounts.vault_state.to_account_info(),
        };
        let cpi_ctx = CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            cpi_accounts,
            signer_seeds,
        );
        token::transfer(cpi_ctx, dust)?;
    }

    // Close the prize_pool ATA, reclaiming its rent to admin.
    let cpi_close = CloseAccount {
        account: ctx.accounts.prize_pool.to_account_info(),
        destination: ctx.accounts.admin.to_account_info(),
        authority: ctx.accounts.vault_state.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_close,
        signer_seeds,
    );
    token::close_account(cpi_ctx)?;

    msg!(
        "Contest closed. dust swept: {}, prize_pool ATA closed.",
        dust
    );
    Ok(())
}
