use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{
    VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, Season,
    MAX_CURRENCIES,
};
use crate::errors::VaultError;

/// `enter_contest` — generic single-canonical entry handler (v0.16).
///
/// The user signs an SPL transfer from their ATA → the chosen currency's
/// operator-revenue ATA. Creates a ContestEntry PDA, awards seeds from the
/// contest's bound Season, increments stat counters.
///
/// Replaces v0.15.1's `enter_contest_direct` AND the managed `enter_contest`.
/// The single path works for both web3 (Phantom signs) and web2 (Rails signs
/// with the user's encrypted keypair) — what matters is that the user's
/// keypair signs the transaction.
///
/// Auth: user signs (controls source ATA). 1-of-3 vault signer pays rent
/// (OPSEC-024 — gates the routine op).
///
/// Paused: rejected with VaultPaused while the vault is paused.
///
/// VaultState is zero-copy (v0.16). All field reads go through
/// `vault_state.load()?`.
#[derive(Accounts)]
#[instruction(entry_num: u32, currency_idx: u8)]
pub struct EnterContest<'info> {
    /// Admin pays rent for the ContestEntry PDA.
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The user's wallet — signs the token transfer.
    #[account(mut)]
    pub user: Signer<'info>,

    /// Per-user stat counter PDA.
    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    // OPSEC-024: gate the payer to a vault signer.
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
    pub contest: Box<Account<'info, Contest>>,

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

    /// The mint corresponding to `currency_idx`. Anchor's `token::mint`
    /// constraints on the ATAs already gate this, but we still pull it in
    /// so the user's ATA + op_rev_ata can be checked against the same mint.
    pub currency_mint: Box<Account<'info, Mint>>,

    /// User's ATA for the chosen currency.
    #[account(
        mut,
        token::mint = currency_mint,
        token::authority = user,
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,

    /// Operator-revenue ATA for the chosen currency.
    /// PDA-seed-bind via [b"op_rev", currency_mint.key()].
    #[account(
        mut,
        seeds = [b"op_rev", currency_mint.key().as_ref()],
        bump,
        token::mint = currency_mint,
    )]
    pub op_rev_ata: Box<Account<'info, TokenAccount>>,

    /// Pinned-season seed schedule (OPSEC-023).
    #[account(
        seeds = [b"season", contest.season_id.to_le_bytes().as_ref()],
        bump = season.bump,
    )]
    pub season: Account<'info, Season>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_enter_contest(
    ctx: Context<EnterContest>,
    entry_num: u32,
    currency_idx: u8,
) -> Result<()> {
    let idx = currency_idx as usize;
    require!(idx < MAX_CURRENCIES, VaultError::InvalidCurrencyIndex);

    // Scope the vault load so we drop the Ref before re-borrowing accounts.
    let (slot_mint, slot_op_rev, slot_active, paused) = {
        let vault = ctx.accounts.vault_state.load()?;
        let slot = vault.accepted_currencies[idx];
        (slot.mint, slot.op_rev_ata, slot.active, vault.paused)
    };

    // Pause guard.
    require!(paused == 0, VaultError::VaultPaused);

    // Derived time-lock: reject entries once the contest's lock timestamp has
    // passed. `lock_timestamp == 0` means no lock is scheduled. This is the
    // authoritative lock — Rails enforces an advisory copy for UX only.
    let lock_ts = ctx.accounts.contest.lock_timestamp;
    if lock_ts != 0 {
        require!(
            Clock::get()?.unix_timestamp < lock_ts,
            VaultError::ContestLocked
        );
    }

    // Slot must be in use (registered).
    require!(slot_mint != Pubkey::default(), VaultError::InvalidCurrencyIndex);
    // Slot must be active.
    require!(slot_active == 1, VaultError::CurrencyNotActive);
    // Mint passed in must match the registered slot's mint.
    require!(
        slot_mint == ctx.accounts.currency_mint.key(),
        VaultError::InvalidMint
    );
    // Defense in depth above the PDA-seed bind: op_rev_ata key matches
    // the registered ATA.
    require!(
        slot_op_rev == ctx.accounts.op_rev_ata.key(),
        VaultError::InvalidMint
    );

    let contest = &mut ctx.accounts.contest;

    // Contest must accept this currency (non-zero fee for this slot).
    let entry_fee = contest.entry_fee_by_currency[idx];
    require!(entry_fee > 0, VaultError::EntryFeeNotSet);

    // SPL transfer: user ATA → op_rev ATA.
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.op_rev_ata.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, entry_fee)?;

    // Update contest state.
    contest.entry_fees[idx] = contest.entry_fees[idx]
        .checked_add(entry_fee)
        .ok_or(VaultError::Overflow)?;
    contest.current_entries = contest
        .current_entries
        .checked_add(1)
        .ok_or(VaultError::Overflow)?;

    // Award seeds + bump user counters.
    let user_account = &mut ctx.accounts.user_account;
    let season = &ctx.accounts.season;
    let seed_idx = (entry_num as usize).min(4);
    user_account.seeds = user_account
        .seeds
        .checked_add(season.seed_schedule[seed_idx])
        .ok_or(VaultError::Overflow)?;
    user_account.entries = user_account
        .entries
        .checked_add(1)
        .ok_or(VaultError::Overflow)?;

    // Initialize the entry.
    let entry = &mut ctx.accounts.contest_entry;
    entry.contest_id = contest.contest_id;
    entry.wallet = ctx.accounts.user.key();
    entry.entry_num = entry_num;
    entry.status = EntryStatus::Active;
    entry.rank = 0;
    entry.payout = 0;
    entry.currency_idx = currency_idx;
    entry.bump = ctx.bumps.contest_entry;
    entry._reserved = [0; 16];

    msg!(
        "Entry {} for wallet {} in contest. currency_idx: {}, fee: {}, total entries: {}",
        entry_num,
        entry.wallet,
        currency_idx,
        entry_fee,
        contest.current_entries
    );
    Ok(())
}
