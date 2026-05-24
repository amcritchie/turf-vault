use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `pause` — emergency stop for user-facing funds operations.
///
/// While the vault is paused, these instructions return `VaultPaused`:
///   - deposit
///   - withdraw
///   - enter_contest, enter_contest_direct
///   - enter_contest_with_token, enter_contest_direct_with_token
///
/// These instructions REMAIN AVAILABLE during a pause (intentional —
/// operators may need to wind down in-flight state, mint tokens for
/// already-paid Stripe purchases, etc):
///   - settle_contest, close_contest
///   - mint_entry_token
///   - migrate_user_account, set_username, create_user_account
///   - create_season, create_contest
///   - update_signers, force_close_vault
///   - pause, unpause
///
/// Auth: 2-of-3 (same level as settle_contest / update_signers). One
/// signer wakes the bot; the other (Alex / Mason) cosigns from Phantom.
///
/// `reason` is a human-readable note logged on-chain. It's a fixed
/// 64-byte array for predictable account sizing; trim trailing zeros
/// when displaying.
#[derive(Accounts)]
pub struct PauseVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.validate_multisig(&admin.key(), &cosigner.key())
            @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,
}

pub fn handle_pause(ctx: Context<PauseVault>, reason: [u8; 64]) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    vault.paused = true;

    // Best-effort UTF-8 decode for the log line. If reason contains non-UTF-8
    // bytes, fall back to a hex preview so we still record SOMETHING.
    let trimmed: Vec<u8> = reason.iter().take_while(|b| **b != 0).copied().collect();
    let reason_str = String::from_utf8(trimmed.clone())
        .unwrap_or_else(|_| format!("<binary {} bytes>", trimmed.len()));

    msg!(
        "Vault PAUSED by {} + {}. Reason: {}",
        ctx.accounts.admin.key(),
        ctx.accounts.cosigner.key(),
        reason_str
    );
    Ok(())
}
