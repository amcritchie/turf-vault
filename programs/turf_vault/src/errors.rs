use anchor_lang::prelude::*;

/// Every error this program can return. Codes 6000+ are Anchor's
/// reserved range for program-defined errors; the numbering here is
/// stable across versions (don't renumber — Rails error-decoding maps
/// integer codes to messages).
#[error_code]
pub enum VaultError {
    #[msg("Only the vault admin can perform this action")]
    Unauthorized,
    #[msg("Token mint is not accepted by this vault")]
    InvalidMint,
    #[msg("Insufficient balance for this operation")]
    InsufficientBalance,
    #[msg("Contest is not open for entries")]
    ContestNotOpen,
    #[msg("Contest is full")]
    ContestFull,
    #[msg("Contest has not been settled")]
    ContestNotSettled,
    #[msg("Contest is already settled")]
    ContestAlreadySettled,
    #[msg("User already entered this contest with this entry number")]
    DuplicateEntry,
    #[msg("Settlement payouts exceed entry fees plus prizes")]
    SettlementOverflow,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Payout amounts must sum to prizes amount")]
    InvalidPayoutTiers,
    #[msg("Account is already larger than expected — cannot migrate")]
    AccountAlreadyMigrated,
    #[msg("Account data is invalid or has wrong discriminator")]
    InvalidAccountData,
    #[msg("Invalid threshold: must be 1-3")]
    InvalidThreshold,
    #[msg("Duplicate signer in signers array")]
    DuplicateSigner,
    #[msg("Entry token has already been consumed")]
    EntryTokenAlreadyConsumed,
    #[msg("Entry token owner does not match the wallet entering the contest")]
    EntryTokenWrongOwner,
    #[msg("New signer set must retain at least one current cosigner")]
    SignerContinuityRequired,
    // ─── v0.15.0 ──────────────────────────────────────────────────────────
    #[msg("Vault is paused — user-facing funds operations are temporarily disabled")]
    VaultPaused,
    #[msg("Withdrawal would exceed the $100 / 24h per-user cap")]
    WithdrawDailyCapExceeded,
    // ─── v0.15.1 ──────────────────────────────────────────────────────────
    // Prelaunch audit C2 — username validation on create_user_account + set_username.
    #[msg("Username uses a reserved prefix")]
    UsernameReserved,
    #[msg("Username contains characters that are not printable ASCII (0x20..0x7E)")]
    UsernameInvalidChars,
    #[msg("Username must be at least 3 characters")]
    UsernameTooShort,
}
