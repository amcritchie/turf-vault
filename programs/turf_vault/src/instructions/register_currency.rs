use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token, TokenAccount};
use crate::state::{VaultState, AcceptedCurrency, MAX_CURRENCIES};
use crate::errors::VaultError;

/// `register_currency` — add a new currency to the on-chain registry.
///
/// Picks the first empty slot in `accepted_currencies` and writes the
/// AcceptedCurrency entry. Creates the per-currency operator-revenue ATA
/// at [b"op_rev", mint.key()] with vault_state as the token authority.
///
/// Rejects:
///   - Mint already present in the registry (active or deactivated), to
///     preserve currency_idx stability.
///   - Registry full (all 16 slots occupied).
///
/// Auth: 2-of-3 multisig. Adding a currency is a treasury-level operation
/// since it expands the surface that operator revenue can flow through.
///
/// VaultState is zero-copy (v0.16). load_mut() for the write.
#[derive(Accounts)]
pub struct RegisterCurrency<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        mut,
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    pub mint: Account<'info, Mint>,

    /// Per-currency operator-revenue ATA. Anchor's `init` rejects this if
    /// the PDA already exists, which gives us defense in depth against
    /// re-registering a previously-deactivated mint (the ATA still exists).
    #[account(
        init,
        payer = admin,
        token::mint = mint,
        token::authority = vault_state,
        seeds = [b"op_rev", mint.key().as_ref()],
        bump,
    )]
    pub op_rev_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
    pub rent: Sysvar<'info, Rent>,
}

pub fn handle_register_currency(
    ctx: Context<RegisterCurrency>,
    kind: u8,
) -> Result<()> {
    let mint_key = ctx.accounts.mint.key();
    let op_rev_key = ctx.accounts.op_rev_ata.key();

    let mut vault = ctx.accounts.vault_state.load_mut()?;

    // Reject re-adding any mint already in the registry (active or not).
    // This preserves currency_idx stability — once a slot is assigned to a
    // mint, that mapping is permanent.
    for slot in vault.accepted_currencies.iter() {
        require!(
            slot.mint != mint_key,
            VaultError::CurrencyAlreadyRegistered
        );
    }

    // Find the first unused slot (mint == Pubkey::default()).
    let mut chosen: Option<usize> = None;
    for (i, slot) in vault.accepted_currencies.iter().enumerate() {
        if slot.mint == Pubkey::default() {
            chosen = Some(i);
            break;
        }
    }
    let slot_idx = chosen.ok_or(VaultError::CurrencyRegistryFull)?;
    require!(slot_idx < MAX_CURRENCIES, VaultError::CurrencyRegistryFull);

    vault.accepted_currencies[slot_idx] = AcceptedCurrency {
        mint: mint_key,
        op_rev_ata: op_rev_key,
        kind,
        active: 1,
        _pad: [0; 14],
    };

    msg!(
        "Currency registered: slot={}, mint={}, op_rev_ata={}, kind={}",
        slot_idx,
        mint_key,
        op_rev_key,
        kind
    );
    Ok(())
}
