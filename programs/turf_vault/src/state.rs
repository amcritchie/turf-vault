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

/// Maximum number of currencies in the on-chain registry. Capped at 16 to
/// keep `accepted_currencies` at 1280 bytes (16 × 80) and `entry_fee_by_currency`
/// / `entry_fees` at 128 bytes (16 × 8) each.
pub const MAX_CURRENCIES: usize = 16;

/// Max payout tiers per contest. Mirrors the v0.15.1 `#[max_len(10)]`.
pub const MAX_PAYOUT_TIERS: usize = 10;

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

/// USDC mint pinned by a mainnet build (canonical Circle USDC). Used to
/// pin both `vault_state.payout_mint` AND `accepted_currencies[0].mint`.
/// Bytes = base58 decode of `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`.
#[cfg(feature = "mainnet")]
pub const EXPECTED_USDC_MINT: Pubkey = Pubkey::new_from_array([
    198, 250, 122, 243, 190, 219, 173, 58, 61, 101, 243, 106, 171, 201, 116, 49,
    177, 187, 228, 194, 210, 246, 224, 228, 124, 166, 2, 3, 69, 47, 93, 97,
]);

/// USDT mint pinned by a mainnet build (canonical Tether USDT). Used to
/// pin `accepted_currencies[1].mint`. (USDT is not the payout mint in v0.16;
/// it's just the second pre-registered entry-fee currency.)
/// Bytes = base58 decode of `Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB`.
#[cfg(feature = "mainnet")]
pub const EXPECTED_USDT_MINT: Pubkey = Pubkey::new_from_array([
    206, 1, 14, 96, 175, 237, 178, 39, 23, 189, 99, 25, 47, 84, 20, 90,
    63, 150, 90, 51, 187, 130, 210, 199, 2, 158, 178, 206, 30, 32, 130, 100,
]);

// ──────────────────────────────────────────────────────────────────────────
// AcceptedCurrency
// ──────────────────────────────────────────────────────────────────────────

/// One slot in the on-chain currency registry. `mint == Pubkey::default()`
/// means the slot is unused. `active == 0` means the slot is registered
/// but disabled for new entries / new contests (slot is never reclaimed —
/// preserves currency_idx stability across the program's life).
///
/// `kind` is an operator tag (0 = stablecoin, 1 = sol-wrapped, etc.) —
/// informational only, never consulted by program logic. `_pad` reserves
/// 14 bytes for future fields (per-currency fee-cap, display ticker, etc.).
///
/// Zero-copy (Pod-safe): `active` is u8 (0/1) rather than bool — bool is
/// not Pod-safe because non-{0,1} byte values cause undefined behavior.
/// We provide unsafe Pod + Zeroable impls so this struct can sit inside
/// `VaultState`'s `accepted_currencies: [AcceptedCurrency; 16]` and be
/// re-interpreted from the account's raw bytes via bytemuck.
#[repr(C)]
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy)]
pub struct AcceptedCurrency {
    /// SPL mint for this currency. `Pubkey::default()` for unused slots.
    pub mint: Pubkey,             // 32
    /// Per-currency operator-revenue ATA (PDA at [b"op_rev", mint]).
    pub op_rev_ata: Pubkey,       // 32
    /// Operator tag (0 = stablecoin, 1 = sol-wrapped, ...). Informational.
    pub kind: u8,                 //  1
    /// 1 == active; 0 == deactivated. Flip to 0 via `deactivate_currency`.
    /// u8 instead of bool for Pod safety (zero-copy compatibility).
    pub active: u8,               //  1
    /// Reserved padding for forward-compat. 14 bytes brings the struct to
    /// 80 bytes total for clean alignment + future field expansion.
    pub _pad: [u8; 14],           // 14
}
// Total: 80 bytes per slot. 16 slots = 1280 bytes.

// SAFETY: AcceptedCurrency is `#[repr(C)]` and every field is itself Pod
// (Pubkey is Pod via solana_program, u8 + u8 + [u8; 14] are trivially Pod).
// No padding bytes — fields pack to exactly 80 bytes (32+32+1+1+14).
unsafe impl anchor_lang::__private::bytemuck::Pod for AcceptedCurrency {}
unsafe impl anchor_lang::__private::bytemuck::Zeroable for AcceptedCurrency {}

// ──────────────────────────────────────────────────────────────────────────
// Accounts
// ──────────────────────────────────────────────────────────────────────────

/// Singleton vault account. Holds the 2-of-3 multisig signer set, the
/// pinned payout mint (USDC), the per-currency registry, a treasury
/// authority pin (Squads vault PDA), and a pause flag.
///
/// PDA seeds: [b"vault"]
///
/// Zero-copy: VaultState is too large (~1475 data bytes) to deserialize
/// onto BPF's 4KB stack via borsh. Anchor's `#[account(zero_copy)]` maps
/// the account data buffer directly as a typed reference via bytemuck::Pod,
/// avoiding the full-struct stack alloc that borsh's deserialize_reader
/// would perform.
///
/// `paused` is u8 (0/1) rather than bool for Pod safety.
#[account(zero_copy(unsafe))]
#[repr(C)]
pub struct VaultState {
    /// The three multisig signers. 1-of-3 can run routine ops; 2-of-3
    /// needed for treasury ops (settle, register/deactivate_currency,
    /// cancel_contest, sweep_operator_revenue, unlock_contest, pause/unpause).
    pub signers: [Pubkey; 3],                          //   96
    /// Number of distinct signatures required for treasury ops. Currently 2.
    pub threshold: u8,                                 //    1
    /// PDA bump.
    pub bump: u8,                                      //    1
    /// Emergency pause flag. When 1, enter_contest and
    /// enter_contest_with_token return VaultPaused. Other ops remain
    /// available so operators can wind down in-flight state.
    /// u8 instead of bool for Pod safety (zero-copy compatibility).
    pub paused: u8,                                    //    1
    /// Payout mint — pinned at `initialize`, immutable thereafter.
    /// All `settle_contest` / `cancel_contest` / `close_contest` flows
    /// constrain this. In a mainnet build, must equal EXPECTED_USDC_MINT.
    pub payout_mint: Pubkey,                           //   32
    /// Squads vault PDA — pinned at `initialize`. `sweep_operator_revenue`
    /// enforces `treasury_ata.owner == treasury_authority`, so leaked admin
    /// keys can't drain swept revenue elsewhere.
    pub treasury_authority: Pubkey,                    //   32
    /// On-chain currency registry. Slot 0 holds the payout currency (USDC),
    /// slot 1 holds USDT, slots 2-15 are populated via `register_currency`.
    pub accepted_currencies: [AcceptedCurrency; 16],   // 1280
    /// Reserved padding for forward-compat (governance, fee policy, etc.).
    pub _reserved: [u8; 64],                           //   64
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

/// One per user wallet. Holds on-chain stat counters (entries / wins /
/// cashes / total_won) plus the loyalty seeds counter and the user's
/// chosen display name. v0.16 dropped the custodial balance / deposit /
/// withdraw / daily-cap fields — funds now live in user ATAs.
///
/// PDA seeds: [b"user", wallet.as_ref()]
#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    /// Owner wallet.
    pub wallet: Pubkey,           // 32
    /// On-chain master copy of the user's display name. UTF-8 zero-padded.
    pub username: [u8; 32],       // 32
    /// Loyalty points awarded per entry from the contest's Season schedule.
    /// Rails derives "level" client-side from this value.
    pub seeds: u64,               //  8
    /// Lifetime contests entered. Increments on every successful entry.
    pub entries: u32,             //  4
    /// Lifetime 1st-place finishes (rank == 1 in settle_contest).
    pub wins: u32,                //  4
    /// Lifetime any-payout finishes (payout > 0 in settle_contest).
    pub cashes: u32,              //  4
    /// Lifetime USDC payouts received. Increments by `payout` on settle.
    pub total_won: u64,           //  8
    /// PDA bump.
    pub bump: u8,                 //  1
    /// Reserved padding for forward-compat (referral, kyc tier, etc.).
    pub _reserved: [u8; 32],      // 32
}

/// Lifecycle of a Contest.
///   Open      — accepting entries
///   Locked    — entries closed, awaiting results (set by lock_contest)
///   Settled   — graded, payouts disbursed
///   Cancelled — refunded to creator, no further state transitions allowed
#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum ContestStatus {
    Open,
    Locked,
    Settled,
    Cancelled,
}

/// One per contest. Holds the per-currency entry-fee schedule, accumulated
/// per-currency fee tallies (operator revenue, separate from prize pool),
/// payout tiers, status, season binding, and the creator/admin pubkeys.
///
/// PDA seeds: [b"contest", contest_id]  (contest_id = SHA256 of Rails slug)
#[account]
#[derive(InitSpace)]
pub struct Contest {
    /// SHA256(Rails slug). Stable, opaque, 32 bytes.
    pub contest_id: [u8; 32],
    /// Payer pubkey from create_contest (admin bot, pays SOL rent).
    pub admin: Pubkey,
    /// Creator pubkey (signs the prize_pool USDC transfer from their ATA).
    pub creator: Pubkey,
    /// Season this contest is bound to (OPSEC-023). Pins the seed schedule.
    pub season_id: u32,
    /// USDC prize pool — pre-funded by creator at create time. Immutable.
    pub prize_pool: u64,
    /// Per-currency entry-fee schedule. `entry_fee_by_currency[i] = 0`
    /// means currency `i` is NOT accepted for this contest.
    pub entry_fee_by_currency: [u64; MAX_CURRENCIES],
    /// Per-currency collected fees (operator revenue). Increments by the
    /// fee on every paid entry of currency `i`.
    pub entry_fees: [u64; MAX_CURRENCIES],
    /// Maximum number of entries this contest accepts.
    pub max_entries: u32,
    /// Number of entries currently in the contest.
    pub current_entries: u32,
    /// Current lifecycle state.
    pub status: ContestStatus,
    /// USDC paid per rank, 6-decimal. Must sum to `prize_pool`. Max 10 ranks.
    #[max_len(10)]
    pub payout_amounts: Vec<u64>,
    /// PDA bump.
    pub bump: u8,
    /// Reserved padding for forward-compat.
    pub _reserved: [u8; 32],
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
    pub contest_id: [u8; 32],     // 32
    pub wallet: Pubkey,           // 32
    pub entry_num: u32,           //  4
    pub status: EntryStatus,      //  1
    pub rank: u32,                //  4
    pub payout: u64,              //  8
    /// Index into `vault_state.accepted_currencies` of the currency this
    /// entry was paid in. `u8::MAX` is the sentinel for token-funded entries
    /// (no SPL transfer occurred).
    pub currency_idx: u8,         //  1
    pub bump: u8,                 //  1
    pub _reserved: [u8; 16],      // 16
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
