use anchor_lang::prelude::*;
use crate::state::{
    VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, EntryTokenAccount, Season,
};
use crate::errors::VaultError;

/// `enter_contest_with_token` — entry funded by consuming an
/// EntryTokenAccount instead of paying any currency.
///
/// Same as `enter_contest` minus the SPL transfer + per-currency tally
/// increment. Awards seeds, bumps user counters, marks the entry as
/// token-funded via `currency_idx = u8::MAX` (sentinel — see §11 Q2).
///
/// Auth: user signs (consents to token consumption — OPSEC-004).
/// 1-of-3 vault signer pays rent (OPSEC-024).
///
/// Paused: rejected with VaultPaused while the vault is paused.
///
/// VaultState is zero-copy (v0.16) — `vault_state.load()?` for reads.
#[derive(Accounts)]
#[instruction(entry_num: u32)]
pub struct EnterContestWithToken<'info> {
    /// Admin pays rent for the ContestEntry PDA.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The user's wallet — signs to authorize token consumption (OPSEC-004).
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
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&payer.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    // H1 prelaunch audit: PDA-seed-bind Contest.
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

    /// Pinned-season seed schedule (OPSEC-023).
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
    // Pause guard. Scope the Ref so it drops before other borrows.
    {
        let vault = ctx.accounts.vault_state.load()?;
        require!(vault.paused == 0, VaultError::VaultPaused);
    }

    // Derived time-lock (see enter_contest). `lock_timestamp == 0` = no lock.
    let lock_ts = ctx.accounts.contest.lock_timestamp;
    if lock_ts != 0 {
        require!(
            Clock::get()?.unix_timestamp < lock_ts,
            VaultError::ContestLocked
        );
    }

    let contest = &mut ctx.accounts.contest;
    let entry_token = &mut ctx.accounts.entry_token;
    let season = &ctx.accounts.season;
    let user_account = &mut ctx.accounts.user_account;

    // No SPL transfer. The token IS the payment.
    contest.current_entries = contest
        .current_entries
        .checked_add(1)
        .ok_or(VaultError::Overflow)?;

    // Award seeds + bump user counters.
    let seed_idx = (entry_num as usize).min(4);
    user_account.seeds = user_account
        .seeds
        .checked_add(season.seed_schedule[seed_idx])
        .ok_or(VaultError::Overflow)?;
    user_account.entries = user_account
        .entries
        .checked_add(1)
        .ok_or(VaultError::Overflow)?;

    // Consume the entry token.
    let now = Clock::get()?.unix_timestamp;
    entry_token.consumed = true;
    entry_token.consumed_at = Some(now);

    // Initialize the entry — currency_idx = u8::MAX (token-funded sentinel).
    let entry = &mut ctx.accounts.contest_entry;
    entry.contest_id = contest.contest_id;
    entry.wallet = ctx.accounts.user.key();
    entry.entry_num = entry_num;
    entry.status = EntryStatus::Active;
    entry.rank = 0;
    entry.payout = 0;
    entry.currency_idx = u8::MAX;
    entry.bump = ctx.bumps.contest_entry;
    entry._reserved = [0; 16];

    msg!(
        "Token-funded entry {} for wallet {}. Token consumed at {}. Current entries: {}",
        entry_num,
        entry.wallet,
        now,
        contest.current_entries
    );
    Ok(())
}
