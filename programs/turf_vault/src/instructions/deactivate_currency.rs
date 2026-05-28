use anchor_lang::prelude::*;
use crate::state::{VaultState, MAX_CURRENCIES};
use crate::errors::VaultError;

/// `deactivate_currency` — flip a slot's `active` flag to 0.
///
/// The slot itself is preserved (mint + op_rev_ata stay). This ensures
/// historical `Contest.entry_fee_by_currency[idx]` and
/// `Contest.entry_fees[idx]` remain valid lookups.
///
/// Side effects:
///   - Existing open contests with non-zero fee at this slot will fail
///     entry with `CurrencyNotActive`.
///   - `create_contest` will reject any new contest with non-zero fee
///     at this slot (validation #4 in §3.7).
///   - The operator-revenue ATA for the deactivated currency stays
///     drainable via `sweep_operator_revenue`.
///
/// Auth: 2-of-3 multisig.
///
/// VaultState is zero-copy (v0.16). load_mut() for the write.
#[derive(Accounts)]
pub struct DeactivateCurrency<'info> {
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
}

pub fn handle_deactivate_currency(
    ctx: Context<DeactivateCurrency>,
    currency_idx: u8,
) -> Result<()> {
    let idx = currency_idx as usize;
    require!(idx < MAX_CURRENCIES, VaultError::InvalidCurrencyIndex);

    let mut vault = ctx.accounts.vault_state.load_mut()?;

    require!(
        vault.accepted_currencies[idx].mint != Pubkey::default(),
        VaultError::InvalidCurrencyIndex
    );
    require!(
        vault.accepted_currencies[idx].active == 1,
        VaultError::CurrencyNotActive
    );

    vault.accepted_currencies[idx].active = 0;
    let mint = vault.accepted_currencies[idx].mint;

    msg!(
        "Currency deactivated: slot={}, mint={}",
        idx,
        mint
    );
    Ok(())
}
