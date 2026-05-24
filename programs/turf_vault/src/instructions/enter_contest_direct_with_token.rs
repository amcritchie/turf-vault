use anchor_lang::prelude::*;
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, EntryTokenAccount, Season};
use crate::errors::VaultError;

/// `enter_contest_direct_with_token` — Phantom-wallet entry funded by a
/// pre-purchased EntryTokenAccount instead of USDC.
///
/// Same as `enter_contest_direct` but consumes one of the user's
/// EntryTokenAccount PDAs atomically (sets consumed=true). No USDC
/// transfer; no `contest.entry_fees` increment. Seeds still awarded.
///
/// Auth: user signs (consents to token consumption). Admin pays SOL rent
/// (any wallet can be payer — gating on `payer` is not necessary because
/// the user's consent IS the authorization).
///
/// Paused: rejected with VaultPaused while the vault is paused.
#[derive(Accounts)]
#[instruction(entry_num: u32)]
pub struct EnterContestDirectWithToken<'info> {
    /// Admin pays rent for the ContestEntry PDA
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The user's Phantom wallet — signs to authorize token consumption
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

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
            user.key().as_ref(),
            &entry_num.to_le_bytes(),
        ],
        bump,
    )]
    pub contest_entry: Account<'info, ContestEntry>,

    /// EntryTokenAccount being consumed to fund this entry.
    /// Must be owned by `user` and not yet consumed.
    #[account(
        mut,
        constraint = entry_token.owner == user.key() @ VaultError::EntryTokenWrongOwner,
        constraint = !entry_token.consumed @ VaultError::EntryTokenAlreadyConsumed,
    )]
    pub entry_token: Account<'info, EntryTokenAccount>,

    /// Season whose seed_schedule controls per-entry seed awards.
    /// OPSEC-023: seeds-pinned to the contest's bound season so a caller
    /// can't substitute a richer-reward season.
    #[account(
        seeds = [b"season", contest.season_id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,

    pub system_program: Program<'info, System>,
}

pub fn handle_enter_contest_direct_with_token(
    ctx: Context<EnterContestDirectWithToken>,
    entry_num: u32,
) -> Result<()> {
    // v0.15.0: emergency pause guard.
    require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

    let contest = &mut ctx.accounts.contest;
    let entry_token = &mut ctx.accounts.entry_token;
    let season = &ctx.accounts.season;

    // NOTE: No USDC transfer occurs. The token IS the payment.
    // `contest.entry_fees` is NOT incremented (no fee collected).
    contest.current_entries = contest.current_entries.checked_add(1).ok_or(VaultError::Overflow)?;

    // Award seeds from the season schedule (token entries still progress the user's level)
    // Entries 5+ clamp to slot 4.
    let user_account = &mut ctx.accounts.user_account;
    let idx = (entry_num as usize).min(4);
    user_account.seeds = user_account.seeds.checked_add(season.seed_schedule[idx]).ok_or(VaultError::Overflow)?;

    // Consume the entry token
    let now = Clock::get()?.unix_timestamp;
    entry_token.consumed = true;
    entry_token.consumed_at = Some(now);

    // Create entry
    let entry = &mut ctx.accounts.contest_entry;
    entry.contest_id = contest.contest_id;
    entry.wallet = ctx.accounts.user.key();
    entry.entry_num = entry_num;
    entry.status = EntryStatus::Active;
    entry.rank = 0;
    entry.payout = 0;
    entry.bump = ctx.bumps.contest_entry;

    msg!(
        "Token-funded direct entry {} for wallet {}. Token consumed at {}. Current entries: {}",
        entry_num,
        entry.wallet,
        now,
        contest.current_entries
    );
    Ok(())
}
