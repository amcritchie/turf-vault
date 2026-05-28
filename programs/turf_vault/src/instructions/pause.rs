use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `pause` — emergency stop for user-facing funds operations.
///
/// While the vault is paused, these instructions return `VaultPaused`:
///   - enter_contest
///   - enter_contest_with_token
///
/// These instructions REMAIN AVAILABLE during a pause (intentional —
/// operators may need to wind down in-flight state, mint tokens for
/// already-paid Stripe purchases, etc):
///   - settle_contest, close_contest, cancel_contest
///   - mint_entry_token
///   - register_currency, deactivate_currency
///   - set_username, create_user_account
///   - create_season, create_contest, lock_contest, unlock_contest
///   - sweep_operator_revenue
///   - pause, unpause
///
/// Auth: 2-of-3 (same level as settle_contest). One signer wakes the bot;
/// the other (Alex / Mason) cosigns from Phantom.
///
/// `reason` is a human-readable note logged on-chain. It's a fixed
/// 64-byte array for predictable account sizing; trim trailing zeros
/// when displaying.
///
/// VaultState is zero-copy (v0.16). load_mut() for the write.
#[derive(Accounts)]
pub struct PauseVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.validate_multisig(&admin.key(), &cosigner.key())
            @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,
}

pub fn handle_pause(ctx: Context<PauseVault>, reason: [u8; 64]) -> Result<()> {
    let admin_key = ctx.accounts.admin.key();
    let cosigner_key = ctx.accounts.cosigner.key();

    {
        let mut vault = ctx.accounts.vault_state.load_mut()?;
        vault.paused = 1;
    }

    // Best-effort UTF-8 decode for the log line. If reason contains non-UTF-8
    // bytes, fall back to a hex preview so we still record SOMETHING.
    let trimmed: Vec<u8> = reason.iter().take_while(|b| **b != 0).copied().collect();
    let reason_str = String::from_utf8(trimmed.clone())
        .unwrap_or_else(|_| format!("<binary {} bytes>", trimmed.len()));

    msg!(
        "Vault PAUSED by {} + {}. Reason: {}",
        admin_key,
        cosigner_key,
        reason_str
    );
    Ok(())
}
