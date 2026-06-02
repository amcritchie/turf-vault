use anchor_lang::prelude::*;

/// Every error this program can return. Codes 6000+ are Anchor's
/// reserved range for program-defined errors; the numbering here is
/// stable across versions (don't renumber — Rails error-decoding maps
/// integer codes to messages).
///
/// v0.16 retired codes (kept for numbering stability — variants stay
/// in the enum but no path emits them):
///   - 6011 AccountAlreadyMigrated (retired with v0.15.1)
///   - 6012 InvalidAccountData (was reserved)
///   - 6019 WithdrawDailyCapExceeded (no more daily cap)
///
/// v0.20 un-retired 6017 SignerContinuityRequired — `update_signers` is back.
///
/// v0.16 new codes start at 6023.
#[error_code]
pub enum VaultError {
    // ── 6000-6022: stable from v0.15.1 ────────────────────────────────────
    #[msg("Only the vault admin can perform this action")]
    Unauthorized,                                // 6000
    #[msg("Token mint is not accepted by this vault")]
    InvalidMint,                                 // 6001
    #[msg("Insufficient balance for this operation")]
    InsufficientBalance,                         // 6002
    #[msg("Contest is not open for entries")]
    ContestNotOpen,                              // 6003
    #[msg("Contest is full")]
    ContestFull,                                 // 6004
    #[msg("Contest has not been settled")]
    ContestNotSettled,                           // 6005
    #[msg("Contest is already settled")]
    ContestAlreadySettled,                       // 6006
    #[msg("User already entered this contest with this entry number")]
    DuplicateEntry,                              // 6007
    #[msg("Settlement payouts exceed prize pool")]
    SettlementOverflow,                          // 6008
    #[msg("Arithmetic overflow")]
    Overflow,                                    // 6009
    #[msg("Payout amounts must sum to prize_pool amount")]
    InvalidPayoutTiers,                          // 6010
    #[msg("Account is already larger than expected — cannot migrate")]
    AccountAlreadyMigrated,                      // 6011 — retired
    #[msg("Account data is invalid or has wrong discriminator")]
    InvalidAccountData,                          // 6012 — retired
    #[msg("Invalid threshold: must be 1-3")]
    InvalidThreshold,                            // 6013
    #[msg("Duplicate signer in signers array")]
    DuplicateSigner,                             // 6014
    #[msg("Entry token has already been consumed")]
    EntryTokenAlreadyConsumed,                   // 6015
    #[msg("Entry token owner does not match the wallet entering the contest")]
    EntryTokenWrongOwner,                        // 6016
    #[msg("New signer set must retain BOTH authorizing cosigners (2-of-3) and contain no default/zeroed slots")]
    SignerContinuityRequired,                    // 6017 — live again in v0.20 (update_signers)
    #[msg("Vault is paused — user-facing funds operations are temporarily disabled")]
    VaultPaused,                                 // 6018
    #[msg("Withdrawal would exceed the per-user daily cap")]
    WithdrawDailyCapExceeded,                    // 6019 — retired
    #[msg("Username uses a reserved prefix")]
    UsernameReserved,                            // 6020
    #[msg("Username contains characters that are not printable ASCII (0x20..0x7E)")]
    UsernameInvalidChars,                        // 6021
    #[msg("Username must be at least 3 characters")]
    UsernameTooShort,                            // 6022

    // ── 6023+: new in v0.16 ───────────────────────────────────────────────
    #[msg("Currency mint is already registered in the vault registry")]
    CurrencyAlreadyRegistered,                   // 6023
    #[msg("Vault currency registry is full")]
    CurrencyRegistryFull,                        // 6024
    #[msg("Currency index is out of range or refers to an unused slot")]
    InvalidCurrencyIndex,                        // 6025
    #[msg("Currency is registered but deactivated")]
    CurrencyNotActive,                           // 6026
    #[msg("Contest does not accept this currency (entry fee is zero)")]
    EntryFeeNotSet,                              // 6027
    #[msg("Contest is not locked")]
    ContestNotLocked,                            // 6028
    #[msg("Contest cannot be cancelled in its current status")]
    ContestNotCancellable,                       // 6029
    #[msg("Prize pool account still holds tokens — cannot close contest")]
    PrizePoolNotEmpty,                           // 6030
    #[msg("Operator-revenue account is empty — nothing to sweep")]
    EmptyRevenueAccount,                         // 6031
    #[msg("Treasury ATA owner does not match the pinned treasury authority")]
    TreasuryAuthorityMismatch,                   // 6032
    #[msg("Contest must have at least one entry fee or a non-zero prize pool")]
    FeeAndPrizeBothZero,                         // 6033
    #[msg("Contest is locked — the lock timestamp has passed")]
    ContestLocked,                               // 6034
    #[msg("Contest has concluded — the conclusion timestamp has passed")]
    ContestConcluded,                            // 6035

    // ── 6036+: new in v0.19 (audit highs #3 / #5) ─────────────────────────
    #[msg("Settlement payout destination is not the winner's associated token account")]
    InvalidPayoutDestination,                    // 6036
    #[msg("Invalid contest timestamp: must be non-negative, in the future, and lock must precede conclusion")]
    InvalidTimestamp,                            // 6037
    #[msg("Entry token seed hash does not match sha256(source_ref)")]
    EntryTokenSeedMismatch,                      // 6038
}
