use anchor_lang::prelude::*;

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
}
