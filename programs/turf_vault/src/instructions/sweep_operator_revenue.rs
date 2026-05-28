use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::VaultState;
use crate::errors::VaultError;

/// `sweep_operator_revenue` — drain a per-currency operator-revenue ATA to
/// the pinned treasury wallet's ATA. Per-currency (one mint per call) to
/// keep the instruction small and avoid variable-length-account headaches.
///
/// Auth: 2-of-3 multisig.
///
/// Validations:
///   - multisig (constraint).
///   - op_rev_ata.amount > 0 (else `EmptyRevenueAccount`).
///   - `amount` <= op_rev_ata.amount (or 0 for sweep-all).
///   - treasury_ata.owner == vault_state.treasury_authority (§11 Q3).
///   - treasury_ata.mint == currency_mint (constraint).
///
/// Side effects:
///   - SPL Transfer (PDA-signed): op_rev_ata → treasury_ata.
///
/// VaultState is zero-copy (v0.16) — load()? for treasury_authority + bump.
#[derive(Accounts)]
pub struct SweepOperatorRevenue<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    pub currency_mint: Box<Account<'info, Mint>>,

    /// Operator-revenue ATA for the given currency.
    /// PDA-bound to [b"op_rev", currency_mint.key()].
    #[account(
        mut,
        seeds = [b"op_rev", currency_mint.key().as_ref()],
        bump,
        token::mint = currency_mint,
        token::authority = vault_state,
    )]
    pub op_rev_ata: Box<Account<'info, TokenAccount>>,

    /// Destination ATA — treasury wallet's ATA for this currency.
    /// `treasury_ata.owner` is verified in the handler against
    /// `vault_state.treasury_authority` (Squads vault PDA).
    #[account(
        mut,
        token::mint = currency_mint,
    )]
    pub treasury_ata: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_sweep_operator_revenue(
    ctx: Context<SweepOperatorRevenue>,
    amount: u64,
) -> Result<()> {
    let available = ctx.accounts.op_rev_ata.amount;
    require!(available > 0, VaultError::EmptyRevenueAccount);

    // Load once to pull both bump and treasury_authority, then drop the
    // Ref before CPI (CPI grabs AccountInfo, which conflicts with a held Ref).
    let (vault_bump, treasury_authority) = {
        let vault = ctx.accounts.vault_state.load()?;
        (vault.bump, vault.treasury_authority)
    };

    // Verify the treasury ATA owner matches the pinned treasury authority.
    require!(
        ctx.accounts.treasury_ata.owner == treasury_authority,
        VaultError::TreasuryAuthorityMismatch
    );

    let to_send = if amount == 0 {
        available
    } else {
        require!(amount <= available, VaultError::InsufficientBalance);
        amount
    };

    let vault_seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&vault_bump)];
    let signer_seeds = &[vault_seeds];

    let cpi_accounts = Transfer {
        from: ctx.accounts.op_rev_ata.to_account_info(),
        to: ctx.accounts.treasury_ata.to_account_info(),
        authority: ctx.accounts.vault_state.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    token::transfer(cpi_ctx, to_send)?;

    msg!(
        "Operator revenue swept. mint: {}, amount: {}, to treasury: {}",
        ctx.accounts.currency_mint.key(),
        to_send,
        ctx.accounts.treasury_ata.key()
    );
    Ok(())
}
