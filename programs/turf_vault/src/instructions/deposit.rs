use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, UserAccount};
use crate::errors::VaultError;

/// `deposit` — user sends USDC or USDT into the vault.
///
/// The user signs an SPL token Transfer from their own ATA to the vault's
/// token PDA. UserAccount.balance is credited by the same amount. lifetime
/// total_deposited accumulates.
///
/// Auth: the user owns the keypair signing this. For managed (web2) wallets
/// the Rails server holds the keypair and signs on behalf of the user.
///
/// Paused: rejected with VaultPaused while the vault is paused (v0.15.0).
#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        constraint = mint.key() == vault_state.usdc_mint || mint.key() == vault_state.usdt_mint @ VaultError::InvalidMint,
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        token::mint = mint,
        token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        constraint = (mint.key() == vault_state.usdc_mint && vault_token_account.key() == vault_state.vault_usdc)
            || (mint.key() == vault_state.usdt_mint && vault_token_account.key() == vault_state.vault_usdt)
            @ VaultError::InvalidMint,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

pub fn handle_deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
    // v0.15.0: emergency pause guard.
    require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

    // Transfer tokens from user → vault via CPI.
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.vault_token_account.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, amount)?;

    // Credit balance + accumulate lifetime stat.
    let user = &mut ctx.accounts.user_account;
    user.balance = user.balance.checked_add(amount).ok_or(VaultError::Overflow)?;
    user.total_deposited = user
        .total_deposited
        .checked_add(amount)
        .ok_or(VaultError::Overflow)?;

    msg!("Deposited {} to user {}", amount, user.wallet);
    Ok(())
}
