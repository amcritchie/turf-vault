use anchor_lang::prelude::*;
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, EntryTokenAccount, Season};
use crate::errors::VaultError;

/// `enter_contest_with_token` — managed entry funded by a pre-purchased
/// EntryTokenAccount instead of USDC.
///
/// Same as `enter_contest` but atomically consumes one of the user's
/// EntryTokenAccount PDAs (sets consumed=true). No USDC charge,
/// `contest.entry_fees` is NOT incremented (operator subsidizes the prize
/// pool for token-funded entries). Seeds still awarded.
///
/// Auth: 1-of-3 vault signer (payer) AND the wallet itself co-signs. The
/// wallet co-sign (OPSEC-004) closes the "compromised admin key burns a
/// user's token without consent" attack — Rails server holds the managed
/// wallet's keypair and co-signs.
///
/// Paused: rejected with VaultPaused while the vault is paused.
#[derive(Accounts)]
#[instruction(entry_num: u32)]
pub struct EnterContestWithToken<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The wallet that owns the user account + the entry token being consumed.
    /// OPSEC-004: now a required Signer. Previously an UncheckedAccount, which
    /// let any 1-of-3 vault signer (e.g. a compromised Alex Bot key) burn ANY
    /// user's entry token without the owner's consent. For managed (web2)
    /// wallets the server co-signs with the user's custodial keypair — so a
    /// leaked admin key alone is no longer sufficient to consume tokens.
    pub wallet: Signer<'info>,

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

    // H1 prelaunch audit (2026-05-24): PDA-seed-bind Contest. See
    // enter_contest.rs for the attack scenario this closes.
    #[account(
        mut,
        seeds = [b"contest", contest.contest_id.as_ref()],
        bump = contest.bump,
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

pub fn handle_enter_contest_with_token(
    ctx: Context<EnterContestWithToken>,
    entry_num: u32,
) -> Result<()> {
    // v0.15.0: emergency pause guard.
    require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

    let user = &mut ctx.accounts.user_account;
    let contest = &mut ctx.accounts.contest;
    let entry_token = &mut ctx.accounts.entry_token;
    let season = &ctx.accounts.season;

    // NOTE: No USDC entry fee is debited. The token IS the payment.
    // `contest.entry_fees` is NOT incremented (no fee collected).
    contest.current_entries = contest.current_entries.checked_add(1).ok_or(VaultError::Overflow)?;

    // Award seeds from the season schedule (token entries still progress the user's level)
    // Entries 5+ clamp to slot 4.
    let idx = (entry_num as usize).min(4);
    user.seeds = user.seeds.checked_add(season.seed_schedule[idx]).ok_or(VaultError::Overflow)?;

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
