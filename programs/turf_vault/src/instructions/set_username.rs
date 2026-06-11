use anchor_lang::prelude::*;
use crate::state::UserAccount;
use crate::errors::VaultError;

/// `set_username` — wallet owner sets / overwrites their display username.
///
/// The on-chain UserAccount holds the master copy of the username (v0.14.0);
/// Rails mirrors it. The account owner signs the change — no admin,
/// no multisig, no rate limit. Idempotent.
///
/// Bytes are stored verbatim as a 32-byte zero-padded UTF-8 array. v0.15.1
/// (prelaunch audit C2) enforces a minimum on-chain validity bar — reserved
/// prefixes, printable-ASCII, length floor — so a caller bypassing Rails
/// can't claim "admin", inject control chars, or set a 1-char username.
/// Uniqueness + homoglyph normalization stay off-chain in Rails.
#[derive(Accounts)]
pub struct SetUsername<'info> {
    pub wallet: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user", wallet.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.wallet == wallet.key() @ VaultError::Unauthorized,
    )]
    pub user_account: Account<'info, UserAccount>,
}

pub fn handle_set_username(ctx: Context<SetUsername>, username: [u8; 32]) -> Result<()> {
    validate_username(&username)?;

    let user = &mut ctx.accounts.user_account;
    user.username = username;

    msg!("Username updated for: {}", user.wallet);
    Ok(())
}

// Prelaunch audit C2 (2026-05-24): on-chain username validity bar.
// Shared between `set_username` and `create_user_account` so a direct
// program caller can't bypass Rails' format/reservation checks on either
// entry point.
//
// Off-chain (Rails) still owns uniqueness, homoglyph normalization, and
// rate limiting — those are policy/UX concerns, not consensus concerns.
// A `UsernameRegistry` PDA with on-chain uniqueness + per-account rate
// limit is the v0.16 follow-up.

/// Reserved username prefixes — case-INSENSITIVE comparison against the
/// leading bytes of the username. Anything starting with one of these is
/// rejected; protects operator/staff/brand identity from squatting.
const RESERVED_PREFIXES: &[&[u8]] = &[
    b"admin",
    b"system",
    b"turf",
    b"vault",
    b"turfmonster",
    b"support",
    b"mod",
    b"official",
    b"staff",
    b"team",
    b"root",
];

const MIN_USERNAME_LEN: usize = 3;

/// Validates a 32-byte username buffer. Returns Ok(()) iff:
///   - At least MIN_USERNAME_LEN non-null leading bytes.
///   - Every non-null byte is printable ASCII (0x20..=0x7E).
///   - The leading bytes (lowercased) do NOT start with a reserved prefix.
///
/// Trailing 0x00 bytes are treated as standard fixed-array padding and
/// allowed (the "effective length" is the index of the first 0x00, or 32).
///
/// v0.25: split into `validate_username_charset_len` + `validate_username_prefix`
/// so the admin-authorized variants (`admin_create_user_account`,
/// `admin_set_username`) can waive ONLY the reserved-prefix branch — a vault
/// signer co-signature never relaxes the charset / min-length bar.
pub fn validate_username(username: &[u8; 32]) -> Result<()> {
    validate_username_charset_len(username)?;
    validate_username_prefix(username)
}

/// Charset + length floor only — the non-waivable half of the validity bar.
/// Used directly by the admin-authorized username instructions.
pub fn validate_username_charset_len(username: &[u8; 32]) -> Result<()> {
    let effective_len = effective_len(username);
    require!(effective_len >= MIN_USERNAME_LEN, VaultError::UsernameTooShort);

    // Charset: every non-null byte must be printable ASCII (0x20..=0x7E).
    // This rejects control chars, high-bit bytes, and non-ASCII UTF-8 —
    // the latter is also how homoglyph attacks are kept out at this layer
    // (Rails normalizes further before this point in the legit flow).
    for &b in &username[..effective_len] {
        require!(
            (0x20..=0x7E).contains(&b),
            VaultError::UsernameInvalidChars
        );
    }

    Ok(())
}

/// Reserved-prefix check only — the half a 1-of-3 vault signer may waive via
/// the admin-authorized instructions (the operator's own "turf" house account
/// is the canonical case).
pub fn validate_username_prefix(username: &[u8; 32]) -> Result<()> {
    let effective_len = effective_len(username);

    // Reserved prefix: lowercase compare against each entry.
    for reserved in RESERVED_PREFIXES {
        if effective_len >= reserved.len() {
            let head = &username[..reserved.len()];
            let matches = head
                .iter()
                .zip(reserved.iter())
                .all(|(a, b)| a.eq_ignore_ascii_case(b));
            if matches {
                return Err(error!(VaultError::UsernameReserved));
            }
        }
    }

    Ok(())
}

/// Index of the first 0x00 (fixed-array padding), or 32 if none.
fn effective_len(username: &[u8; 32]) -> usize {
    username.iter().position(|&b| b == 0).unwrap_or(32)
}
