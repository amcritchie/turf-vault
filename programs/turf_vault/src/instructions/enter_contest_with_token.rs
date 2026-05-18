use anchor_lang::prelude::*;
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, EntryTokenAccount};
use crate::errors::VaultError;

/// Managed entry funded by an EntryTokenAccount (no USDC fee charged).
/// Same as `enter_contest` but consumes a pre-purchased entry token in the
/// same atomic transaction. User balance is NOT debited; seeds are still awarded.
///
/// Auth: any 1-of-3 vault signer (same routine-op pattern as enter_contest).
#[derive(Accounts)]
#[instruction(entry_num: u32)]
pub struct EnterContestWithToken<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The wallet that owns the user account (may differ from payer for custodial).
    /// CHECK: Validated via user_account PDA seeds; also must match entry_token.owner.
    pub wallet: UncheckedAccount<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.is_signer(&payer.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"user", wallet.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        mut,
        constraint = contest.status == ContestStatus::Open @ VaultError::ContestNotOpen,
        constraint = contest.current_entries < contest.max_entries @ VaultError::ContestFull,
    )]
    pub contest: Account<'info, Contest>,

    #[account(
        init,
        payer = payer,
        space = 8 + ContestEntry::INIT_SPACE,
        seeds = [
            b"entry",
            contest.contest_id.as_ref(),
            wallet.key().as_ref(),
            &entry_num.to_le_bytes(),
        ],
        bump,
    )]
    pub contest_entry: Account<'info, ContestEntry>,

    /// EntryTokenAccount being consumed to fund this entry.
    /// Must be owned (logically) by `wallet` and not yet consumed.
    #[account(
        mut,
        constraint = entry_token.owner == wallet.key() @ VaultError::EntryTokenWrongOwner,
        constraint = !entry_token.consumed @ VaultError::EntryTokenAlreadyConsumed,
    )]
    pub entry_token: Account<'info, EntryTokenAccount>,

    pub system_program: Program<'info, System>,
}

pub fn handle_enter_contest_with_token(
    ctx: Context<EnterContestWithToken>,
    entry_num: u32,
) -> Result<()> {
    let user = &mut ctx.accounts.user_account;
    let contest = &mut ctx.accounts.contest;
    let entry_token = &mut ctx.accounts.entry_token;

    // NOTE: No USDC entry fee is debited. The token IS the payment.
    // `contest.entry_fees` is NOT incremented (no fee collected).
    contest.current_entries = contest.current_entries.checked_add(1).ok_or(VaultError::Overflow)?;

    // Award 65 seeds (token entries still progress the user's level)
    user.seeds = user.seeds.checked_add(65).ok_or(VaultError::Overflow)?;

    // Consume the entry token
    let now = Clock::get()?.unix_timestamp;
    entry_token.consumed = true;
    entry_token.consumed_at = Some(now);

    // Create entry
    let entry = &mut ctx.accounts.contest_entry;
    entry.contest_id = contest.contest_id;
    entry.wallet = ctx.accounts.wallet.key();
    entry.entry_num = entry_num;
    entry.status = EntryStatus::Active;
    entry.rank = 0;
    entry.payout = 0;
    entry.bump = ctx.bumps.contest_entry;

    msg!(
        "Token-funded entry {} for wallet {}. Token consumed at {}. Current entries: {}",
        entry_num,
        entry.wallet,
        now,
        contest.current_entries
    );
    Ok(())
}
