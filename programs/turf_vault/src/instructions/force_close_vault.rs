use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// Migration-only instruction: closes the old VaultState account so it can be
/// re-initialized with the new schema.
///
/// We cannot use `Account<'info, VaultState>` because the on-chain data has the
/// OLD layout. Instead we accept a raw UncheckedAccount, verify the PDA seeds,
/// read the first signer pubkey from raw bytes, confirm the signer matches,
/// and then close the account by draining its lamports.
///
/// Requires 2-of-3 multisig: both admin and cosigner must be stored signers.
#[derive(Accounts)]
pub struct ForceCloseVault<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    /// The existing vault account (old layout). We verify PDA seeds manually.
    /// CHECK: Verified via seeds check and manual signer comparison below.
    #[account(
        mut,
        seeds = [b"vault"],
        bump,
    )]
    pub vault_state: UncheckedAccount<'info>,
}

pub fn handle_force_close_vault(ctx: Context<ForceCloseVault>) -> Result<()> {
    let vault_info = &ctx.accounts.vault_state;
    let data = vault_info.try_borrow_data()?;

    // Layout: 8-byte discriminator + 3 x 32-byte signers = 104 bytes minimum
    // We read all 3 signers and verify both admin and cosigner are in the array
    require!(data.len() >= 104, VaultError::Unauthorized);

    // OPSEC-026: force_close is migration-ONLY — it exists to close a vault
    // whose on-chain layout is STALE (different size from the current
    // schema) so it can be re-initialized. Refuse to run against a vault
    // that's already at the current schema size: there's nothing to
    // migrate, and closing it would brick a healthy program. Without this
    // guard the instruction was replayable forever — 2 compromised signers
    // could DoS the live vault at any time.
    require!(
        data.len() != 8 + VaultState::INIT_SPACE,
        VaultError::AccountAlreadyMigrated
    );

    let signer0 = Pubkey::try_from(&data[8..40])
        .map_err(|_| error!(VaultError::Unauthorized))?;
    let signer1 = Pubkey::try_from(&data[40..72])
        .map_err(|_| error!(VaultError::Unauthorized))?;
    let signer2 = Pubkey::try_from(&data[72..104])
        .map_err(|_| error!(VaultError::Unauthorized))?;

    let stored_signers = [signer0, signer1, signer2];
    let admin_key = ctx.accounts.admin.key();
    let cosigner_key = ctx.accounts.cosigner.key();

    // Validate 2-of-3: both must be different and both must be stored signers
    require!(admin_key != cosigner_key, VaultError::Unauthorized);
    require!(stored_signers.contains(&admin_key), VaultError::Unauthorized);
    require!(stored_signers.contains(&cosigner_key), VaultError::Unauthorized);

    // Drop borrow before modifying lamports
    drop(data);

    // Close the account: drain lamports to admin
    let vault_lamports = vault_info.lamports();
    **vault_info.try_borrow_mut_lamports()? = 0;
    **ctx.accounts.admin.try_borrow_mut_lamports()? = ctx
        .accounts
        .admin
        .lamports()
        .checked_add(vault_lamports)
        .ok_or(VaultError::Overflow)?;

    // Zero out account data so runtime garbage-collects it
    let mut data = vault_info.try_borrow_mut_data()?;
    data.fill(0);

    msg!("Vault account force-closed. Rent reclaimed by admin.");
    Ok(())
}
