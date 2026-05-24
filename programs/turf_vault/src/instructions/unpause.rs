use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `unpause` — lift the emergency stop.
///
/// Restores deposit / withdraw / enter_contest* operations. Same 2-of-3
/// auth as `pause` — flipping the switch off needs the same authority
/// as flipping it on. No time-based auto-unpause (deliberately — an
/// attacker who can pause should not be able to wait it out).
#[derive(Accounts)]
pub struct UnpauseVault<'info> {
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

pub fn handle_unpause(ctx: Context<UnpauseVault>) -> Result<()> {
    let vault = &mut ctx.accounts.vault_state;
    vault.paused = false;

    msg!(
        "Vault UNPAUSED by {} + {}",
        ctx.accounts.admin.key(),
        ctx.accounts.cosigner.key()
    );
    Ok(())
}
