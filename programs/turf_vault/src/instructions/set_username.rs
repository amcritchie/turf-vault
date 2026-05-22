use anchor_lang::prelude::*;
use crate::state::UserAccount;
use crate::errors::VaultError;

/// Sets the username on a UserAccount. The username's master record lives
/// on-chain (v0.14.0); the Rails app mirrors it. The account owner signs —
/// no admin or multisig involvement. Idempotent: overwrites any prior value.
///
/// The signing `wallet` must own the `user_account` PDA. Username bytes are
/// stored verbatim (UTF-8, zero-padded); format + uniqueness are enforced by
/// the Rails app before this is called.
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
