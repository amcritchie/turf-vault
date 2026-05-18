use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct VaultState {
    pub signers: [Pubkey; 3],  // All three multisig signers
    pub threshold: u8,         // Required sigs for treasury ops (2)
    pub usdc_mint: Pubkey,
    pub usdt_mint: Pubkey,
    pub vault_usdc: Pubkey,
    pub vault_usdt: Pubkey,
    pub bump: u8,
}

impl VaultState {
    /// Any signer can perform routine ops (single-signer)
    pub fn is_signer(&self, key: &Pubkey) -> bool {
        self.signers.contains(key)
    }

    /// Treasury ops require threshold distinct signers
    pub fn validate_multisig(&self, s1: &Pubkey, s2: &Pubkey) -> bool {
        s1 != s2 && self.is_signer(s1) && self.is_signer(s2)
    }
}

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    pub wallet: Pubkey,
    pub balance: u64,          // 6 decimals (USDC/USDT native)
    pub total_deposited: u64,
    pub total_withdrawn: u64,
    pub total_won: u64,
    pub seeds: u64,            // 60 seeds awarded per contest entry
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum ContestStatus {
    Open,
    Locked,
    Settled,
}

#[account]
#[derive(InitSpace)]
pub struct Contest {
    pub contest_id: [u8; 32],  // SHA256 of Rails slug
    pub prizes: u64,           // guaranteed prize amount admin pre-funds
    pub entry_fee: u64,        // 6 decimals
    pub entry_fees: u64,       // accumulated entry fees collected
    pub max_entries: u32,
    pub current_entries: u32,
    pub status: ContestStatus,
    #[max_len(10)]
    pub payout_amounts: Vec<u64>,  // USDC amounts per rank (6 decimals, e.g. [40_000000])
    pub admin: Pubkey,
    pub creator: Pubkey,       // wallet that funded the prizes USDC
    pub bump: u8,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Copy, PartialEq, Eq, InitSpace)]
pub enum EntryStatus {
    Active,
    Won,
    Lost,
}

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

/// Source of an entry token (how the user obtained it)
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
