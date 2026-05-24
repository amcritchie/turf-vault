use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `update_signers` — rotate the multisig signer set or change threshold.
///
/// Use cases: a signer's key was lost / compromised; you want to move a
/// signer to a hardware wallet; you want to add a third trust domain.
///
/// Auth: 2-of-3 of the CURRENT signers.
///
/// OPSEC-027 continuity: at least one of the two cosigners authorizing
/// this update must remain in the new signer set. Without this guard, two
/// compromised signers could rotate to three attacker-controlled
/// addresses, locking out the legitimate third party with no recovery path.
#[derive(Accounts)]
pub struct UpdateSigners<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,
}

pub fn handle_update_signers(ctx: Context<UpdateSigners>, new_signers: [Pubkey; 3], new_threshold: u8) -> Result<()> {
    require!(new_threshold >= 1 && new_threshold <= 3, VaultError::InvalidThreshold);
    require!(new_signers[0] != new_signers[1] && new_signers[0] != new_signers[2] && new_signers[1] != new_signers[2], VaultError::DuplicateSigner);

    // OPSEC-027: require continuity — at least one of the two cosigners who
    // authorized THIS update must remain in the new set. Without this, two
    // compromised signers could rotate to three attacker-controlled
    // addresses, locking out the legitimate third party with no recovery
    // path. Continuity guarantees a known-good key always survives a
    // rotation (and catches a fat-fingered Phantom paste that would
    // otherwise brick the multisig).
    let admin_key = ctx.accounts.admin.key();
    let cosigner_key = ctx.accounts.cosigner.key();
    require!(
        new_signers.contains(&admin_key) || new_signers.contains(&cosigner_key),
        VaultError::SignerContinuityRequired
    );

    let vault = &mut ctx.accounts.vault_state;
    vault.signers = new_signers;
    vault.threshold = new_threshold;

    msg!("Signers updated. New signers: [{}, {}, {}], Threshold: {}", new_signers[0], new_signers[1], new_signers[2], new_threshold);
    Ok(())
}
