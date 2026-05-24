use anchor_lang::prelude::*;
use crate::state::UserAccount;
use crate::errors::VaultError;

/// `set_username` — wallet owner sets / overwrites their display username.
///
/// The on-chain UserAccount holds the master copy of the username (v0.14.0);
/// Rails mirrors it. The account owner signs the change — no admin,
/// no multisig, no rate limit. Idempotent.
///
/// Bytes are stored verbatim as a 32-byte zero-padded UTF-8 array. Format
/// and uniqueness are NOT enforced on-chain — Rails handles that before
/// calling. Multiple wallets could in principle share a username on-chain;
/// off-chain consumers should treat (wallet, username) as the identity.
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
    let user = &mut ctx.accounts.user_account;
    user.username = username;

    msg!("Username updated for: {}", user.wallet);
    Ok(())
}
