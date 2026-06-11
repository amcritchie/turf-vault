use anchor_lang::prelude::*;
use crate::state::{UserAccount, VaultState};
use crate::errors::VaultError;
use crate::instructions::set_username::validate_username_charset_len;

/// `admin_set_username` — `set_username` with an admin-authorized
/// reserved-prefix waiver (v0.25).
///
/// Identical semantics to `set_username` (the account OWNER signs —
/// consenting; no admin can rename a user unilaterally) PLUS a required
/// `admin` co-signer that must be a 1-of-3 vault signer
/// (`vault_state.is_signer`, else `Unauthorized` 6000).
///
/// The admin co-signature waives ONLY the reserved-prefix branch of the
/// v0.15.1 username validity bar (audit C2) — charset (printable ASCII) and
/// the 3-char length floor are still enforced. Canonical use: renaming the
/// operator's house account to "turf", which the plain path rejects with
/// `UsernameReserved` 6020.
///
/// VaultState is zero-copy (v0.16) — load()? for the signer check. Read-only.
#[derive(Accounts)]
pub struct AdminSetUsername<'info> {
    pub wallet: Signer<'info>,

    /// Vault signer (1-of-3) authorizing the reserved-prefix waiver.
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"user", wallet.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.wallet == wallet.key() @ VaultError::Unauthorized,
    )]
    pub user_account: Account<'info, UserAccount>,
}

pub fn handle_admin_set_username(
    ctx: Context<AdminSetUsername>,
    username: [u8; 32],
) -> Result<()> {
    // Reserved-prefix check waived (admin co-signed); charset + min-length
    // are NOT waivable on any path.
    validate_username_charset_len(&username)?;

    let user = &mut ctx.accounts.user_account;
    user.username = username;

    msg!(
        "Username updated (admin-authorized) for: {} by admin: {}",
        user.wallet,
        ctx.accounts.admin.key()
    );
    Ok(())
}
