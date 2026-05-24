//! TurfVault — Solana escrow program for Turf Monster contests.
//!
//! High-level model:
//!   - **VaultState** is the singleton holding signers, mints, and a
//!     pause flag.
//!   - **UserAccount** is per-wallet — holds balance, lifetime accounting,
//!     loyalty seeds, and a daily withdraw circuit-breaker.
//!   - **Contest** holds entry fee, prize pool, and payout tiers; bound
//!     to one **Season** (seed schedule).
//!   - **ContestEntry** is per (contest × wallet × entry_num).
//!   - **EntryTokenAccount** is a pre-purchased free-entry voucher.
//!
//! Auth model:
//!   - **1-of-3 vault signer**: routine ops (create_contest, mint_entry_token,
//!     facilitate entries, migrate_user_account).
//!   - **2-of-3 vault signers**: treasury ops (settle_contest, force_close_vault,
//!     update_signers, pause, unpause).
//!   - **User signature**: deposit, withdraw, direct entries, set_username.
//!   - **INIT_AUTHORITY constant** (Alex Phantom key): one-time `initialize` call.
//!
//! For each instruction's full contract, see its file under `instructions/`.

use anchor_lang::prelude::*;

pub mod errors;
pub mod state;
pub mod instructions;

use instructions::*;

declare_id!("Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT");

#[program]
pub mod turf_vault {
    use super::*;

    // ── Vault setup & governance ──────────────────────────────────────────

    /// One-time setup of the singleton vault. Only callable by INIT_AUTHORITY
    /// with the canonical USDC + USDT mints for this build.
    pub fn initialize(ctx: Context<Initialize>, signers: [Pubkey; 3], threshold: u8) -> Result<()> {
        handle_initialize(ctx, signers, threshold)
    }

    /// Close the existing VaultState account to enable a schema migration.
    /// Requires 2-of-3. Only succeeds if the on-chain data is at a DIFFERENT
    /// size than the current schema (won't brick a healthy vault).
    pub fn force_close_vault(ctx: Context<ForceCloseVault>) -> Result<()> {
        handle_force_close_vault(ctx)
    }

    /// Rotate the multisig signer set or change the threshold. 2-of-3
    /// required; at least one of the two cosigners must remain in the new
    /// set (OPSEC-027 continuity).
    pub fn update_signers(
        ctx: Context<UpdateSigners>,
        new_signers: [Pubkey; 3],
        new_threshold: u8,
    ) -> Result<()> {
        handle_update_signers(ctx, new_signers, new_threshold)
    }

    // ── Pause control (v0.15.0) ───────────────────────────────────────────

    /// Emergency stop: blocks deposit / withdraw / enter_contest*. 2-of-3.
    /// `reason` is logged on-chain (UTF-8 zero-padded to 64 bytes).
    pub fn pause(ctx: Context<PauseVault>, reason: [u8; 64]) -> Result<()> {
        handle_pause(ctx, reason)
    }

    /// Lift the emergency stop. 2-of-3.
    pub fn unpause(ctx: Context<UnpauseVault>) -> Result<()> {
        handle_unpause(ctx)
    }

    // ── User accounts ─────────────────────────────────────────────────────

    /// Create a new per-wallet UserAccount PDA. Permissionless payer (any
    /// wallet can pay rent for any user's account); the username is set at
    /// creation time and can later be changed via `set_username`.
    pub fn create_user_account(
        ctx: Context<CreateUserAccount>,
        wallet: Pubkey,
        username: [u8; 32],
    ) -> Result<()> {
        handle_create_user_account(ctx, wallet, username)
    }

    /// Resize a UserAccount PDA to the current v0.15.0 layout. Handles
    /// both v0.13 (81 bytes, pre-username) and v0.14 (113 bytes) source
    /// layouts. Idempotent. Admin-only.
    pub fn migrate_user_account(ctx: Context<MigrateUserAccount>) -> Result<()> {
        handle_migrate_user_account(ctx)
    }

    /// Set / overwrite the username on a UserAccount. Signed by the
    /// account owner — no admin involvement.
    pub fn set_username(ctx: Context<SetUsername>, username: [u8; 32]) -> Result<()> {
        handle_set_username(ctx, username)
    }

    // ── Funds in / out ────────────────────────────────────────────────────

    /// User → vault SPL token transfer. Credits UserAccount.balance.
    /// Blocked when vault is paused.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        handle_deposit(ctx, amount)
    }

    /// Vault → user SPL token transfer. Debits UserAccount.balance.
    /// Capped at $100 per rolling 24h window per user (v0.15.0).
    /// Blocked when vault is paused.
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        handle_withdraw(ctx, amount)
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

    /// Create a new contest. Dual-signer: payer (admin bot, pays SOL rent) +
    /// creator (Phantom wallet, signs prize-pool USDC transfer from their ATA).
    pub fn create_contest(
        ctx: Context<CreateContest>,
        contest_id: [u8; 32],
        season_id: u32,
        entry_fee: u64,
        max_entries: u32,
        payout_amounts: Vec<u64>,
        prizes: u64,
    ) -> Result<()> {
        handle_create_contest(
            ctx,
            contest_id,
            season_id,
            entry_fee,
            max_entries,
            payout_amounts,
            prizes,
        )
    }

    /// Grade a contest: assign ranks + credit payouts to winners' UserAccounts.
    /// 2-of-3 (admin + cosigner). Uses remaining_accounts for variable-length
    /// settlement arrays. Cap: total payouts ≤ entry_fees + prizes.
    pub fn settle_contest(ctx: Context<SettleContest>, settlements: Vec<Settlement>) -> Result<()> {
        handle_settle_contest(ctx, settlements)
    }

    /// Close a settled contest's account, reclaiming rent to the admin.
    /// 1-of-3 vault signer.
    pub fn close_contest(ctx: Context<CloseContest>) -> Result<()> {
        handle_close_contest(ctx)
    }

    // ── Entries ───────────────────────────────────────────────────────────

    /// Managed-wallet entry: debits UserAccount.balance for the entry fee
    /// and creates the ContestEntry. 1-of-3 vault signer pays rent.
    /// Blocked when vault is paused.
    pub fn enter_contest(ctx: Context<EnterContest>, entry_num: u32) -> Result<()> {
        handle_enter_contest(ctx, entry_num)
    }

    /// Phantom-wallet entry: user signs an SPL transfer from their ATA →
    /// vault USDC PDA. Admin pays SOL rent. Blocked when vault is paused.
    pub fn enter_contest_direct(ctx: Context<EnterContestDirect>, entry_num: u32) -> Result<()> {
        handle_enter_contest_direct(ctx, entry_num)
    }

    /// Managed-wallet entry funded by consuming an EntryTokenAccount
    /// (no USDC charged). Wallet must co-sign to consent (OPSEC-004).
    /// Blocked when vault is paused.
    pub fn enter_contest_with_token(
        ctx: Context<EnterContestWithToken>,
        entry_num: u32,
    ) -> Result<()> {
        handle_enter_contest_with_token(ctx, entry_num)
    }

    /// Phantom-wallet entry funded by consuming an EntryTokenAccount.
    /// User signs; admin pays SOL rent. Blocked when vault is paused.
    pub fn enter_contest_direct_with_token(
        ctx: Context<EnterContestDirectWithToken>,
        entry_num: u32,
    ) -> Result<()> {
        handle_enter_contest_direct_with_token(ctx, entry_num)
    }

    // ── Free entries ──────────────────────────────────────────────────────

    /// Mint a new EntryTokenAccount for a user. 1-of-3 vault signer.
    /// `sequence` is supplied by the caller to avoid PDA collisions when
    /// minting multiple tokens per user.
    pub fn mint_entry_token(
        ctx: Context<MintEntryToken>,
        sequence: u64,
        source: u8,
        source_ref: [u8; 64],
    ) -> Result<()> {
        handle_mint_entry_token(ctx, sequence, source, source_ref)
    }
}
