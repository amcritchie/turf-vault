//! TurfVault — Solana escrow program for Turf Monster contests.
//!
//! v0.16 server-signed self-custody model (NOT custodial-balance):
//!   - **VaultState** is the singleton holding signers, payout mint pin,
//!     treasury authority pin, the on-chain currency registry, and a pause flag.
//!   - **UserAccount** is per-wallet — holds on-chain stat counters
//!     (entries, wins, cashes, total_won), loyalty seeds, and username.
//!     Funds live in user ATAs, NOT in the vault.
//!   - **Contest** holds the per-currency entry-fee schedule, a USDC
//!     prize pool (held in a per-contest [b"prize_pool", contest_id] ATA),
//!     payout tiers, and is bound to one **Season** (seed schedule).
//!   - **ContestEntry** is per (contest × wallet × entry_num).
//!   - **EntryTokenAccount** is a pre-purchased free-entry voucher.
//!
//! Auth model:
//!   - **1-of-3 vault signer**: routine ops (create_contest,
//!     set_contest_lock_time, close_contest, mint_entry_token, facilitate
//!     entries).
//!   - **2-of-3 vault signers**: treasury ops (register_currency,
//!     deactivate_currency, settle_contest, cancel_contest,
//!     sweep_operator_revenue, pause, unpause).
//!   - **User signature**: enter_contest{,_with_token}, set_username,
//!     create_contest (as creator funding the prize pool).
//!   - **INIT_AUTHORITY constant** (Alex Phantom key): one-time `initialize` call.
//!
//! For each instruction's full contract, see its file under `instructions/`.

use anchor_lang::prelude::*;

pub mod errors;
pub mod state;
pub mod instructions;

use instructions::*;

// Cluster-gated declare_id!. Devnet/localnet builds compile-time bind to the
// current devnet v0.16 program. Mainnet builds resolve to a placeholder until
// the operator generates the mainnet program keypair and edits this file per
// docs/MAINNET_LAUNCH.md §3.
//
// Anchor.toml's [programs.*] entries are advisory (CLI tooling) and MUST
// agree with whichever declare_id! the active feature set selects.
#[cfg(not(feature = "mainnet"))]
declare_id!("EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ");

#[cfg(feature = "mainnet")]
declare_id!("11111111111111111111111111111111"); // PLACEHOLDER — replace per docs/MAINNET_LAUNCH.md §3

#[program]
pub mod turf_vault {
    use super::*;

    // ── Vault setup ───────────────────────────────────────────────────────

    /// One-time setup of the singleton vault. Pins payout mint to USDC,
    /// registers slot 0 (USDC) + slot 1 (USDT) in the currency registry,
    /// stores the Squads vault PDA as treasury authority, and locks in the
    /// multisig signer set + threshold. Only callable by INIT_AUTHORITY
    /// on a mainnet build.
    pub fn initialize(
        ctx: Context<Initialize>,
        signers: [Pubkey; 3],
        threshold: u8,
        treasury_authority: Pubkey,
    ) -> Result<()> {
        handle_initialize(ctx, signers, threshold, treasury_authority)
    }

    // ── Currency registry ─────────────────────────────────────────────────

    /// Add a currency to `accepted_currencies` at the first empty slot.
    /// Creates the per-currency operator-revenue ATA. 2-of-3.
    pub fn register_currency(ctx: Context<RegisterCurrency>, kind: u8) -> Result<()> {
        handle_register_currency(ctx, kind)
    }

    /// Flip `accepted_currencies[idx].active = 0`. The slot is never
    /// reclaimed — preserves currency_idx stability. 2-of-3.
    pub fn deactivate_currency(
        ctx: Context<DeactivateCurrency>,
        currency_idx: u8,
    ) -> Result<()> {
        handle_deactivate_currency(ctx, currency_idx)
    }

    // ── Pause control ─────────────────────────────────────────────────────

    /// Emergency stop: blocks enter_contest{,_with_token}. 2-of-3. `reason`
    /// is logged on-chain (UTF-8 zero-padded to 64 bytes).
    pub fn pause(ctx: Context<PauseVault>, reason: [u8; 64]) -> Result<()> {
        handle_pause(ctx, reason)
    }

    /// Lift the emergency stop. 2-of-3.
    pub fn unpause(ctx: Context<UnpauseVault>) -> Result<()> {
        handle_unpause(ctx)
    }

    // ── User accounts ─────────────────────────────────────────────────────

    /// Create a new per-wallet UserAccount PDA. Permissionless payer.
    pub fn create_user_account(
        ctx: Context<CreateUserAccount>,
        wallet: Pubkey,
        username: [u8; 32],
    ) -> Result<()> {
        handle_create_user_account(ctx, wallet, username)
    }

    /// Set / overwrite the username on a UserAccount. Owner signs.
    pub fn set_username(ctx: Context<SetUsername>, username: [u8; 32]) -> Result<()> {
        handle_set_username(ctx, username)
    }

    // ── Seasons ───────────────────────────────────────────────────────────

    /// Create a Season with an immutable per-entry seed-award schedule.
    /// 1-of-3 vault signer.
    pub fn create_season(
        ctx: Context<CreateSeason>,
        season_id: u32,
        name: [u8; 32],
        seed_schedule: [u64; 5],
        start_at: i64,
    ) -> Result<()> {
        handle_create_season(ctx, season_id, name, seed_schedule, start_at)
    }

    // ── Contest lifecycle ─────────────────────────────────────────────────

    /// Create a new contest with a per-currency entry-fee schedule and a
    /// USDC prize pool. Dual-signer: payer (admin bot, pays SOL rent) +
    /// creator (Phantom wallet, signs prize-pool USDC transfer).
    pub fn create_contest(
        ctx: Context<CreateContest>,
        contest_id: [u8; 32],
        season_id: u32,
        entry_fee_by_currency: [u64; 16],
        max_entries: u32,
        payout_amounts: Vec<u64>,
        prize_pool: u64,
        lock_timestamp: i64,
    ) -> Result<()> {
        handle_create_contest(
            ctx,
            contest_id,
            season_id,
            entry_fee_by_currency,
            max_entries,
            payout_amounts,
            prize_pool,
            lock_timestamp,
        )
    }

    /// Set (or clear) a contest's derived lock timestamp. 1-of-3.
    /// `new_lock_timestamp == 0` clears the lock (enterable indefinitely); any
    /// non-zero Unix-seconds value locks entries once chain time passes it.
    /// "Lock now" = pass the current chain time. Rejected once the contest is
    /// concluded (interim guard: Settled/Cancelled).
    pub fn set_contest_lock_time(
        ctx: Context<SetContestLockTime>,
        new_lock_timestamp: i64,
    ) -> Result<()> {
        handle_set_contest_lock_time(ctx, new_lock_timestamp)
    }

    /// Set (or clear) a contest's conclusion timestamp (v0.18). 1-of-3. Once
    /// chain time passes it the contest has concluded — set_contest_lock_time
    /// then rejects. `new_conclusion_timestamp == 0` clears it. Rejected once
    /// the contest has already concluded or is settled/cancelled.
    pub fn set_contest_conclusion_time(
        ctx: Context<SetContestConclusionTime>,
        new_conclusion_timestamp: i64,
    ) -> Result<()> {
        handle_set_contest_conclusion_time(ctx, new_conclusion_timestamp)
    }

    /// Grade a contest. Per-winner SPL transfer from the contest's USDC
    /// prize-pool PDA → winner's USDC ATA. 2-of-3.
    pub fn settle_contest<'info>(
        ctx: Context<'_, '_, '_, 'info, SettleContest<'info>>,
        settlements: Vec<Settlement>,
    ) -> Result<()> {
        handle_settle_contest(ctx, settlements)
    }

    /// Refund the prize pool to the creator. Open / Locked → Cancelled. 2-of-3.
    pub fn cancel_contest(ctx: Context<CancelContest>) -> Result<()> {
        handle_cancel_contest(ctx)
    }

    /// Close a settled or cancelled contest's PDA + prize_pool ATA,
    /// reclaiming rent to the admin. Sweeps residual prize_pool dust to
    /// the operator-revenue USDC ATA first (decided 2026-05-27, §11 Q8).
    /// 1-of-3.
    pub fn close_contest(ctx: Context<CloseContest>) -> Result<()> {
        handle_close_contest(ctx)
    }

    // ── Entries ───────────────────────────────────────────────────────────

    /// Generic single-canonical entry handler. User signs an SPL transfer
    /// from their ATA → the chosen currency's operator-revenue ATA. Creates
    /// a ContestEntry PDA, awards seeds, increments stat counters.
    /// Blocked when vault is paused.
    pub fn enter_contest(
        ctx: Context<EnterContest>,
        entry_num: u32,
        currency_idx: u8,
    ) -> Result<()> {
        handle_enter_contest(ctx, entry_num, currency_idx)
    }

    /// Entry funded by consuming an EntryTokenAccount instead of paying
    /// any currency. Wallet co-signs (OPSEC-004). Blocked when vault is paused.
    pub fn enter_contest_with_token(
        ctx: Context<EnterContestWithToken>,
        entry_num: u32,
    ) -> Result<()> {
        handle_enter_contest_with_token(ctx, entry_num)
    }

    // ── Free entries ──────────────────────────────────────────────────────

    /// Mint a new EntryTokenAccount for a user. 1-of-3 vault signer.
    /// PDA is derived from sha256(source_ref) (v0.19, audit #9) — re-minting the
    /// same source_ref collides on init for true on-chain idempotency.
    pub fn mint_entry_token(
        ctx: Context<MintEntryToken>,
        source: u8,
        source_ref: [u8; 64],
        source_ref_hash: [u8; 32],
    ) -> Result<()> {
        handle_mint_entry_token(ctx, source, source_ref, source_ref_hash)
    }

    // ── Treasury ──────────────────────────────────────────────────────────

    /// Drain a per-currency operator-revenue ATA to the pinned treasury
    /// wallet's ATA. `amount = 0` sweeps all. 2-of-3.
    pub fn sweep_operator_revenue(
        ctx: Context<SweepOperatorRevenue>,
        amount: u64,
    ) -> Result<()> {
        handle_sweep_operator_revenue(ctx, amount)
    }
}
