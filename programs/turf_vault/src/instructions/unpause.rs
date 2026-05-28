use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `unpause` — lift the emergency stop.
///
/// Restores enter_contest{,_with_token} operations. Same 2-of-3 auth as
/// `pause` — flipping the switch off needs the same authority as flipping
/// it on. No time-based auto-unpause (deliberately — an attacker who can
/// pause should not be able to wait it out).
///
/// VaultState is zero-copy (v0.16). load_mut() for the write.
#[derive(Accounts)]
pub struct UnpauseVault<'info> {
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

pub fn handle_unpause(ctx: Context<UnpauseVault>) -> Result<()> {
    let admin_key = ctx.accounts.admin.key();
    let cosigner_key = ctx.accounts.cosigner.key();

    {
        let mut vault = ctx.accounts.vault_state.load_mut()?;
        vault.paused = 0;
    }

    msg!(
        "Vault UNPAUSED by {} + {}",
        admin_key,
        cosigner_key
    );
    Ok(())
}
