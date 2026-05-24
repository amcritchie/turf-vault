use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::state::VaultState;
#[cfg(feature = "mainnet")]
use crate::state::{INIT_AUTHORITY, EXPECTED_USDC_MINT, EXPECTED_USDT_MINT};
use crate::errors::VaultError;

/// `initialize` — call once per program deployment.
///
/// Creates the singleton VaultState PDA, the vault's two SPL token accounts
/// (USDC + USDT), and locks in the multisig signer set + threshold.
///
/// Hardening (v0.15.0, mainnet builds only):
///   - admin MUST equal INIT_AUTHORITY (Alex's human Phantom key). Closes
///     the "race the deployer" attack where anyone could call initialize
///     between deploy and the legit init TX.
///   - usdc_mint MUST equal EXPECTED_USDC_MINT for this build (Circle USDC).
///   - usdt_mint MUST equal EXPECTED_USDT_MINT for this build (Tether USDT).
///
/// These three hardening checks are feature-gated to `mainnet`. Default
/// (devnet / localnet) builds skip them so the test suite + dev iteration
/// can create vaults with the local wallet and ad-hoc test mints.
/// `paused` is set to false (vault starts unpaused) in all builds.
///
/// One-time event: after this succeeds, the hardening constants are never
/// re-checked in any other instruction. The vault is then governed by
/// VaultState's signer/threshold configuration.
#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Must be INIT_AUTHORITY in mainnet builds (any signer in dev/test).
    /// Pays SOL rent for VaultState + the two token accounts.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The new VaultState account. PDA at [b"vault"].
    #[account(
        init,
        payer = admin,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [b"vault"],
        bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    /// USDC mint. In mainnet builds, must equal EXPECTED_USDC_MINT.
    pub usdc_mint: Account<'info, Mint>,

    /// USDT mint. In mainnet builds, must equal EXPECTED_USDT_MINT.
    pub usdt_mint: Account<'info, Mint>,

    /// Vault's USDC token account. PDA at [b"vault_usdc"], authority = vault_state.
    #[account(
        init_if_needed,
        payer = admin,
        token::mint = usdc_mint,
        token::authority = vault_state,
        seeds = [b"vault_usdc"],
        bump,
    )]
    pub vault_usdc: Account<'info, TokenAccount>,

    /// Vault's USDT token account. PDA at [b"vault_usdt"], authority = vault_state.
    #[account(
        init_if_needed,
        payer = admin,
        token::mint = usdt_mint,
        token::authority = vault_state,
        seeds = [b"vault_usdt"],
        bump,
    )]
    pub vault_usdt: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    signers: [Pubkey; 3],
    threshold: u8,
) -> Result<()> {
    // v0.15.0 H1: mainnet builds only. Devnet/localnet skip these checks
    // so the test suite can run with locally-generated keys + mints.
    #[cfg(feature = "mainnet")]
    {
        require!(
            ctx.accounts.admin.key() == INIT_AUTHORITY,
            VaultError::Unauthorized
        );
        require!(
            ctx.accounts.usdc_mint.key() == EXPECTED_USDC_MINT,
            VaultError::InvalidMint
        );
        require!(
            ctx.accounts.usdt_mint.key() == EXPECTED_USDT_MINT,
            VaultError::InvalidMint
        );
    }

    // Threshold must be 1, 2, or 3.
    require!(threshold >= 1 && threshold <= 3, VaultError::InvalidThreshold);

    // No duplicate signers in the multisig set.
    require!(
        signers[0] != signers[1] && signers[0] != signers[2] && signers[1] != signers[2],
        VaultError::DuplicateSigner
    );

    // The init authority MUST be in the multisig signer set — otherwise
    // they couldn't sign any subsequent vault op.
    require!(
        signers.contains(&ctx.accounts.admin.key()),
        VaultError::Unauthorized
    );

    let vault = &mut ctx.accounts.vault_state;
    vault.signers = signers;
    vault.threshold = threshold;
    vault.usdc_mint = ctx.accounts.usdc_mint.key();
    vault.usdt_mint = ctx.accounts.usdt_mint.key();
    vault.vault_usdc = ctx.accounts.vault_usdc.key();
    vault.vault_usdt = ctx.accounts.vault_usdt.key();
    vault.bump = ctx.bumps.vault_state;
    vault.paused = false; // v0.15.0: vault starts unpaused.

    msg!(
        "Vault initialized. Signers: [{}, {}, {}], Threshold: {}",
        signers[0],
        signers[1],
        signers[2],
        threshold
    );
    Ok(())
}
