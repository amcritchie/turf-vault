use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::state::{VaultState, AcceptedCurrency};
#[cfg(feature = "mainnet")]
use crate::state::{INIT_AUTHORITY, EXPECTED_USDC_MINT, EXPECTED_USDT_MINT};
use crate::errors::VaultError;

/// `initialize` — call once per program deployment.
///
/// Creates the singleton VaultState PDA, pins the payout mint to USDC,
/// registers USDC (slot 0) and USDT (slot 1) in the currency registry,
/// creates the operator-revenue ATAs for both, stores the Squads vault
/// PDA as the treasury authority, and locks in the multisig signer set
/// + threshold.
///
/// Hardening (mainnet builds only):
///   - admin MUST equal INIT_AUTHORITY (Alex's human Phantom key).
///   - payout_mint (== slot 0) MUST equal EXPECTED_USDC_MINT (Circle USDC).
///   - second_currency_mint (== slot 1) MUST equal EXPECTED_USDT_MINT (Tether).
///
/// These hardening checks are feature-gated to `mainnet`. Default
/// (devnet / localnet) builds skip them so the test suite + dev iteration
/// can create vaults with the local wallet and ad-hoc test mints.
/// `paused` is set to 0 (vault starts unpaused) in all builds.
///
/// One-time event: after this succeeds, the hardening constants are never
/// re-checked in any other instruction. The vault is then governed by
/// VaultState's signer/threshold configuration.
///
/// VaultState is zero-copy (v0.16) — too large for borsh's stack-based
/// deserialization. Use `load_init()?` rather than `load_mut()?` to write
/// the freshly-`init`'d account's discriminator.
#[derive(Accounts)]
pub struct Initialize<'info> {
    /// Must be INIT_AUTHORITY in mainnet builds (any signer in dev/test).
    /// Pays SOL rent for VaultState + the two operator-revenue ATAs.
    #[account(mut)]
    pub admin: Signer<'info>,

    /// The new VaultState account. PDA at [b"vault"].
    /// Zero-copy: size_of<VaultState>() instead of INIT_SPACE.
    #[account(
        init,
        payer = admin,
        space = 8 + std::mem::size_of::<VaultState>(),
        seeds = [b"vault"],
        bump,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    /// Payout mint — pinned to canonical Circle USDC at mainnet build time.
    /// Also registered as `accepted_currencies[0]`.
    pub payout_mint: Account<'info, Mint>,

    /// Second accepted entry-fee currency (USDT). In mainnet builds,
    /// pinned to EXPECTED_USDT_MINT. Registered as `accepted_currencies[1]`.
    pub second_currency_mint: Account<'info, Mint>,

    /// Operator-revenue ATA for the payout currency (USDC).
    /// PDA at [b"op_rev", payout_mint.key()]. Authority = vault_state.
    #[account(
        init,
        payer = admin,
        token::mint = payout_mint,
        token::authority = vault_state,
        seeds = [b"op_rev", payout_mint.key().as_ref()],
        bump,
    )]
    pub payout_op_rev_ata: Account<'info, TokenAccount>,

    /// Operator-revenue ATA for the second currency (USDT).
    /// PDA at [b"op_rev", second_currency_mint.key()]. Authority = vault_state.
    #[account(
        init,
        payer = admin,
        token::mint = second_currency_mint,
        token::authority = vault_state,
        seeds = [b"op_rev", second_currency_mint.key().as_ref()],
        bump,
    )]
    pub second_op_rev_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_initialize(
    ctx: Context<Initialize>,
    signers: [Pubkey; 3],
    threshold: u8,
    treasury_authority: Pubkey,
) -> Result<()> {
    // Mainnet-only hardening checks.
    #[cfg(feature = "mainnet")]
    {
        require!(
            ctx.accounts.admin.key() == INIT_AUTHORITY,
            VaultError::Unauthorized
        );
        require!(
            ctx.accounts.payout_mint.key() == EXPECTED_USDC_MINT,
            VaultError::InvalidMint
        );
        require!(
            ctx.accounts.second_currency_mint.key() == EXPECTED_USDT_MINT,
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

    // The init authority MUST be in the multisig signer set.
    require!(
        signers.contains(&ctx.accounts.admin.key()),
        VaultError::Unauthorized
    );

    // Sanity: the two registered currencies must not be the same mint.
    require!(
        ctx.accounts.payout_mint.key() != ctx.accounts.second_currency_mint.key(),
        VaultError::InvalidMint
    );

    let payout_mint_key = ctx.accounts.payout_mint.key();
    let payout_op_rev_key = ctx.accounts.payout_op_rev_ata.key();
    let second_mint_key = ctx.accounts.second_currency_mint.key();
    let second_op_rev_key = ctx.accounts.second_op_rev_ata.key();
    let vault_bump = ctx.bumps.vault_state;

    // load_init: zero-copy first-time write. load_mut would reject because
    // the account's data is still zeroed (no discriminator yet).
    let mut vault = ctx.accounts.vault_state.load_init()?;
    vault.signers = signers;
    vault.threshold = threshold;
    vault.paused = 0;
    vault.bump = vault_bump;
    vault.payout_mint = payout_mint_key;
    vault.treasury_authority = treasury_authority;

    // Zero out the registry, then pin USDC at slot 0 and USDT at slot 1.
    vault.accepted_currencies = [AcceptedCurrency {
        mint: Pubkey::default(),
        op_rev_ata: Pubkey::default(),
        kind: 0,
        active: 0,
        _pad: [0; 14],
    }; 16];
    vault.accepted_currencies[0] = AcceptedCurrency {
        mint: payout_mint_key,
        op_rev_ata: payout_op_rev_key,
        kind: 0, // stablecoin
        active: 1,
        _pad: [0; 14],
    };
    vault.accepted_currencies[1] = AcceptedCurrency {
        mint: second_mint_key,
        op_rev_ata: second_op_rev_key,
        kind: 0, // stablecoin
        active: 1,
        _pad: [0; 14],
    };
    vault._reserved = [0; 64];

    msg!(
        "Vault initialized. Signers: [{}, {}, {}], Threshold: {}, Payout mint: {}, Treasury: {}",
        signers[0],
        signers[1],
        signers[2],
        threshold,
        payout_mint_key,
        treasury_authority,
    );
    Ok(())
}
