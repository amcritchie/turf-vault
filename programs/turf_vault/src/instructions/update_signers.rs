use anchor_lang::prelude::*;
use crate::state::VaultState;
use crate::errors::VaultError;

/// `update_signers` — rotate the multisig signer set IN PLACE, without a
/// program redeploy.
///
/// Re-added in v0.20 (v0.16 removed the v0.15.x version). The motivating
/// incident: the v0.16 program shipped WITHOUT this instruction, so a
/// deployed program's signer set was immutable — a single compromised signer
/// key (e.g. the Alex Bot server key) could not be rotated in place and forced
/// a full program redeploy + re-init + on-chain rent loss. With this
/// instruction shipped, future signer compromise is a cheap 2-of-3 on-chain
/// transaction, never a redeploy.
///
/// Use cases: a signer's key was lost / compromised; you want to move a signer
/// to a hardware wallet; you want to swap a trust domain.
///
/// THRESHOLD IS PINNED AT 2-of-3 — this instruction rotates signer pubkeys
/// only. `validate_multisig` never reads the `threshold` field (it
/// structurally requires exactly two distinct signers), so a configurable
/// threshold was inert and misleading. The `threshold` field on `VaultState`
/// stays (set once by `initialize`); `update_signers` does NOT touch it.
///
/// Auth: 2-of-3 of the CURRENT signers (`validate_multisig` — two DISTINCT
/// current signers must sign). Server (Alex Bot) partial-signs as `admin`;
/// a human (Alex / Mason) cosigns as `cosigner` from Phantom.
///
/// SAFETY — signer continuity (OPSEC-027). Because governance is 2-of-3, a
/// working multisig needs TWO surviving cosignable keys, not one. The new set
/// must therefore keep BOTH cosigners who authorized THIS update (`admin` AND
/// `cosigner`). A weaker "keep ≥1" guard passes a rotation to
/// `[survivor, junk, junk]` that still bricks all governance (no second key
/// can ever cosign). Requiring both authorizing cosigners to survive
/// guarantees two known-good keys remain — and catches a fat-fingered Phantom
/// paste that would otherwise brick the multisig. The intended "evict the
/// leaked bot key" rotation still passes: Alex + Mason both cosign and both
/// stay. `SignerContinuityRequired` (6017) covers this and the no-default-slot
/// guard.
///
/// VaultState is zero-copy (v0.16+). The `validate_multisig` auth check reads
/// the CURRENT signer set via `load()` in the constraint; the write goes
/// through `load_mut()` in the handler. `signers: [Pubkey; 3]` is a small
/// in-place edit on the existing 1515-byte account — no account-size change,
/// no re-init. `threshold` is left untouched.
#[derive(Accounts)]
pub struct UpdateSigners<'info> {
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

pub fn handle_update_signers(
    ctx: Context<UpdateSigners>,
    new_signers: [Pubkey; 3],
) -> Result<()> {
    // Threshold is pinned at 2-of-3 — this instruction rotates signer pubkeys
    // only and never touches `vault.threshold`. `validate_multisig` ignores
    // the threshold field (it always requires two distinct signers), so there
    // is no threshold to validate or change here.

    // No duplicate keys — a duplicated signer would silently shrink the
    // effective key set below 3 and weaken the multisig (and a 2-of-3 with
    // two identical keys collapses to a 1-of-2). Reuses 6014.
    require!(
        new_signers[0] != new_signers[1]
            && new_signers[0] != new_signers[2]
            && new_signers[1] != new_signers[2],
        VaultError::DuplicateSigner
    );

    // No zeroed / default slots — the set must never contain Pubkey::default()
    // (which would be both a dead key and, paired with the duplicate check, a
    // way to weaken the set). Treat a default slot as a continuity break.
    require!(
        new_signers[0] != Pubkey::default()
            && new_signers[1] != Pubkey::default()
            && new_signers[2] != Pubkey::default(),
        VaultError::SignerContinuityRequired
    );

    // Signer continuity (OPSEC-027): BOTH cosigners who authorized THIS update
    // must remain in the new set. Governance is 2-of-3, so a working multisig
    // needs TWO surviving cosignable keys — a "keep ≥1" guard would pass a
    // rotation to [survivor, junk, junk] that still bricks all governance (no
    // second key could ever cosign). Requiring both `admin` and `cosigner` to
    // survive guarantees two known-good keys remain and catches a fat-fingered
    // Phantom paste. The intended "evict the leaked bot key" rotation still
    // passes: Alex + Mason both cosign and both stay.
    let admin_key = ctx.accounts.admin.key();
    let cosigner_key = ctx.accounts.cosigner.key();
    let keep_admin = new_signers.contains(&admin_key);
    let keep_cosigner = new_signers.contains(&cosigner_key);
    require!(
        keep_admin && keep_cosigner,
        VaultError::SignerContinuityRequired
    );

    let old_signers = {
        let mut vault = ctx.accounts.vault_state.load_mut()?;
        let old_signers = vault.signers;
        vault.signers = new_signers;
        old_signers
    };

    msg!(
        "Signers rotated by {} + {}. Old: [{}, {}, {}] -> New: [{}, {}, {}] (threshold pinned 2-of-3)",
        admin_key,
        cosigner_key,
        old_signers[0],
        old_signers[1],
        old_signers[2],
        new_signers[0],
        new_signers[1],
        new_signers[2]
    );
    Ok(())
}
