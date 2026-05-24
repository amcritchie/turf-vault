use anchor_lang::prelude::*;

// ──────────────────────────────────────────────────────────────────────────
// Program-wide constants (compile-time, immutable)
// ──────────────────────────────────────────────────────────────────────────
//
// These constants harden the vault against the "frontrun-initialize" attack
// class: once the program is deployed, anyone can race the first call to
// `initialize` and become the vault owner. By baking the expected init
// authority and accepted mints into the program itself, an attacker can't
// initialize the vault even if they win the race — Anchor rejects the TX
// before it touches state.
//
// Build profiles:
//   default (devnet) — uses devnet test mints
//   --features mainnet — uses canonical mainnet USDC + USDT mints
// ──────────────────────────────────────────────────────────────────────────

// The H1 hardening constants (INIT_AUTHORITY + EXPECTED_USDC/USDT_MINT)
// are only used by `initialize.rs` under #[cfg(feature = "mainnet")]. They
// are feature-gated here for the same reason — without `mainnet`, they
// don't exist, so a stray reference in non-mainnet code wouldn't compile.

/// The single wallet permitted to call `initialize` on a mainnet build.
/// Hardcoded to Alex's Phantom key — never lives on the server, so a
/// Heroku/AWS compromise can't reinit the vault.
///
/// This matters once-per-deployment. After `initialize` runs, it's never
/// checked again — vault ops use `vault_state.signers` / `validate_multisig`.
/// Operational cost: Alex signs the one-time init TX from Phantom (or a
/// local CLI loaded with their key); the bot takes over for everything else.
///
/// Bytes = base58 decode of `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`.
#[cfg(feature = "mainnet")]
pub const INIT_AUTHORITY: Pubkey = Pubkey::new_from_array([
    97, 102, 159, 134, 4, 135, 247, 38, 25, 80, 120, 222, 238, 244, 126, 240,
    127, 147, 165, 62, 97, 27, 221, 166, 58, 24, 35, 40, 64, 79, 47, 199,
]);

/// USDC mint accepted by a mainnet build (canonical Circle USDC).
/// Bytes = base58 decode of `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
#[cfg(feature = "mainnet")]
pub const EXPECTED_USDC_MINT: Pubkey = Pubkey::new_from_array([
    198, 250, 122, 243, 190, 219, 173, 58, 61, 101, 243, 106, 171, 201, 116, 49,
    177, 187, 228, 194, 210, 246, 224, 228, 124, 166, 2, 3, 69, 47, 93, 97,
]);

/// USDT mint accepted by a mainnet build (canonical Tether USDT).
/// Bytes = base58 decode of `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB`.
#[cfg(feature = "mainnet")]
pub const EXPECTED_USDT_MINT: Pubkey = Pubkey::new_from_array([
    206, 1, 14, 96, 175, 237, 178, 39, 23, 189, 99, 25, 47, 84, 20, 90,
    63, 150, 90, 51, 187, 130, 210, 199, 2, 158, 178, 206, 30, 32, 130, 100,
]);

/// Per-user withdraw cap in a rolling 24-hour window. 6-decimal USDC:
/// 100_000_000 = $100. Creates friction for both legit users and attackers
/// who steal a user's keypair — caps loss-per-day-per-account at $100.
pub const DAILY_WITHDRAW_CAP: u64 = 100_000_000;

/// Rolling-window length for the withdraw cap. 24 hours in seconds.
pub const DAILY_WINDOW_SECONDS: i64 = 86_400;

// ──────────────────────────────────────────────────────────────────────────
// Accounts
// ──────────────────────────────────────────────────────────────────────────

/// Singleton vault account. Holds the 2-of-3 multisig signer set, the two
/// accepted token mints, the vault's token accounts, a bump, and a pause
/// flag (v0.15.0).
///
/// PDA seeds: [b"vault"]
#[account]
#[derive(InitSpace)]
pub struct VaultState {
    /// The three multisig signers. 1-of-3 can run routine ops; 2-of-3
    /// needed for treasury ops (settle, force_close, update_signers,
    /// pause, unpause).
    pub signers: [Pubkey; 3],
    /// Number of distinct signatures required for treasury ops. Currently 2.
    pub threshold: u8,
    /// USDC mint this vault accepts. Set once at init; verified against
    /// EXPECTED_USDC_MINT.
    pub usdc_mint: Pubkey,
    /// USDT mint this vault accepts. Set once at init; verified against
    /// EXPECTED_USDT_MINT.
    pub usdt_mint: Pubkey,
    /// Vault's USDC token account (PDA, authority = this vault_state).
    pub vault_usdc: Pubkey,
    /// Vault's USDT token account (PDA, authority = this vault_state).
    pub vault_usdt: Pubkey,
    /// PDA bump.
    pub bump: u8,
    /// Emergency pause flag (v0.15.0). When true, deposit / withdraw /
    /// enter_contest* are rejected. Settle, close, mint_entry_token,
    /// migrate, set_username, create_user_account, create_season,
    /// update_signers, force_close, pause, unpause remain available.
    /// Flipped via the `pause` / `unpause` instructions (2-of-3).
    pub paused: bool,
}

impl VaultState {
    /// Any signer can perform routine ops (single-signer).
    pub fn is_signer(&self, key: &Pubkey) -> bool {
        self.signers.contains(key)
    }

    /// Treasury ops require `threshold` distinct signers (currently 2-of-3).
    pub fn validate_multisig(&self, s1: &Pubkey, s2: &Pubkey) -> bool {
        s1 != s2 && self.is_signer(s1) && self.is_signer(s2)
    }
}

/// One per user wallet. Holds the user's vault balance plus lifetime
/// accounting and v0.15.0's daily-withdraw circuit breaker.
///
/// PDA seeds: [b"user", wallet.as_ref()]
#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    /// Owner wallet.
    pub wallet: Pubkey,
    /// Current spendable balance, 6-decimal USDC. Credited on deposit and
    /// on `settle_contest` winnings; debited on withdraw and on
    /// `enter_contest` (managed-wallet entry fee).
    pub balance: u64,
    /// Lifetime cumulative deposits. Audit/UX only — never decremented.
    pub total_deposited: u64,
    /// Lifetime cumulative withdrawals. Audit/UX only — never decremented.
    pub total_withdrawn: u64,
    /// Lifetime cumulative winnings credited via settle_contest. Audit/UX only.
    pub total_won: u64,
    /// Loyalty points awarded per entry from the contest's Season schedule.
    /// Rails derives "level" client-side from this value (level = seeds/100 + 1).
    pub seeds: u64,
    /// On-chain master copy of the user's display name (v0.14.0). UTF-8,
    /// zero-padded. Rails mirrors it; only the wallet owner can change via
    /// `set_username`.
    pub username: [u8; 32],
    /// Lamports withdrawn in the current 24h window (v0.15.0). Resets to
    /// zero when DAILY_WINDOW_SECONDS elapses since daily_window_start.
    pub daily_withdrawn: u64,
    /// Unix timestamp of the current window's start (v0.15.0). Zero on
    /// fresh accounts — the first withdraw initializes it.
    pub daily_window_start: i64,
    /// PDA bump.
    pub bump: u8,
}

/// Lifecycle of a Contest.
///   Open    — accepting entries
///   Locked  — entries closed, awaiting results (no instruction sets this yet)
///   Settled — graded, payouts credited
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum ContestStatus {
    Open,
    Locked,
    Settled,
}

/// One per contest. Holds entry-fee schedule, payout tiers, accumulated
/// fees, status, and the season the contest is bound to.
///
/// PDA seeds: [b"contest", contest_id]  (contest_id = SHA256 of Rails slug)
#[account]
#[derive(InitSpace)]
pub struct Contest {
    /// SHA256(Rails slug). Stable, opaque, 32 bytes.
    pub contest_id: [u8; 32],
    /// Guaranteed prize amount the creator pre-funds at create time.
    pub prizes: u64,
    /// Entry fee per entry, 6-decimal USDC.
    pub entry_fee: u64,
    /// Total entry fees collected so far. Increments on every paid entry.
    /// Settlement is capped at `entry_fees + prizes`.
    pub entry_fees: u64,
    /// Maximum number of entries this contest accepts.
    pub max_entries: u32,
    /// Number of entries currently in the contest.
    pub current_entries: u32,
    /// Current lifecycle state.
    pub status: ContestStatus,
    /// USDC paid per rank, 6-decimal. Must sum to `prizes` (validated at
    /// create_contest). Max 10 ranks.
    #[max_len(10)]
    pub payout_amounts: Vec<u64>,
    /// Payer pubkey from create_contest (admin bot, pays SOL rent).
    pub admin: Pubkey,
    /// Creator pubkey (signs the prizes USDC transfer from their ATA).
    pub creator: Pubkey,
    /// Season this contest is bound to (OPSEC-023). Pins the seed schedule
    /// so a caller can't substitute a richer-reward season at entry time.
    pub season_id: u32,
    /// PDA bump.
    pub bump: u8,
}

/// Status of a single contest entry. Settle transitions Active → Won/Lost.
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum EntryStatus {
    Active,
    Won,
    Lost,
}

/// One per (contest, wallet, entry_num). A user can have multiple entries
/// per contest (Rails caps at 3); each gets its own PDA.
///
/// PDA seeds: [b"entry", contest_id, wallet.as_ref(), entry_num.to_le_bytes()]
#[account]
#[derive(InitSpace)]
pub struct ContestEntry {
    pub contest_id: [u8; 32],
    pub wallet: Pubkey,
    pub entry_num: u32,
    pub status: EntryStatus,
    pub rank: u32,
    pub payout: u64,
    pub bump: u8,
}

/// Source enum for an EntryTokenAccount — how the user obtained the token.
pub mod entry_token_source {
    pub const OPERATOR: u8 = 0;
    pub const STRIPE: u8 = 1;
    pub const MOONPAY: u8 = 2;
}

/// EntryTokenAccount: a pre-purchased contest entry token issued by the admin.
/// User redeems one of these to enter a contest without paying the entry fee at entry time.
///
/// PDA seeds: [b"entry_token", owner.as_ref(), sequence.to_le_bytes().as_ref()]
/// Discovery: getProgramAccounts filter by `owner`.
#[account]
pub struct EntryTokenAccount {
    pub owner: Pubkey,            // user wallet
    pub source: u8,               // entry_token_source::{OPERATOR, STRIPE, MOONPAY}
    pub source_ref: [u8; 64],     // truncated/padded external reference (e.g. Stripe session id)
    pub consumed: bool,
    pub consumed_at: Option<i64>, // unix timestamp when consumed
    pub created_at: i64,          // unix timestamp at mint
    pub bump: u8,
}

impl EntryTokenAccount {
    /// Layout (after 8-byte Anchor discriminator):
    ///   owner:        Pubkey      32
    ///   source:       u8           1
    ///   source_ref:   [u8; 64]    64
    ///   consumed:     bool         1
    ///   consumed_at:  Option<i64>  1 + 8 = 9 (tag + payload; payload always present in space calc)
    ///   created_at:   i64          8
    ///   bump:         u8           1
    /// Subtotal data: 116
    /// + 8 discriminator = 124
    pub const LEN: usize = 8 + 32 + 1 + 64 + 1 + (1 + 8) + 8 + 1;
}

/// Season: a contest season with a per-entry seed-award schedule.
/// The schedule is set at season creation and is immutable thereafter.
/// Entries award `seed_schedule[entry_num.min(4) as usize]` — entries 5+
/// clamp to slot 4.
///
/// PDA seeds: [b"season", season_id.to_le_bytes()] (u32 LE, 4 bytes).
#[account]
pub struct Season {
    pub season_id: u32,
    pub name: [u8; 32],          // UTF-8 padded with 0x00
    pub seed_schedule: [u64; 5], // entries 0-4; entry index 5+ clamps to slot 4
    pub start_at: i64,           // unix timestamp
    pub created_at: i64,         // unix timestamp
    pub bump: u8,
}

impl Season {
    /// Layout (after 8-byte Anchor discriminator):
    ///   season_id:      u32          4
    ///   name:           [u8; 32]    32
    ///   seed_schedule:  [u64; 5]    40
    ///   start_at:       i64          8
    ///   created_at:     i64          8
    ///   bump:           u8           1
    /// Subtotal data: 93
    /// + 8 discriminator = 101
    pub const LEN: usize = 8 + 4 + 32 + 40 + 8 + 8 + 1;
}
