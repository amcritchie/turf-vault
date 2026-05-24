use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, UserAccount, DAILY_WITHDRAW_CAP, DAILY_WINDOW_SECONDS};
use crate::errors::VaultError;

/// `withdraw` — user pulls USDC or USDT out of the vault back to their ATA.
///
/// The user signs; the program signs the SPL transfer as the vault_state
/// PDA (the token-account authority). UserAccount.balance is debited;
/// lifetime total_withdrawn accumulates.
///
/// v0.15.0 circuit breaker: limits a single user to DAILY_WITHDRAW_CAP
/// ($100 worth, 6-decimal lamports) per rolling DAILY_WINDOW_SECONDS
/// (24h). The window resets on the first withdraw after 24h since the
/// last window start. This is friction for legit users with large
/// balances AND a kill-switch on stolen managed-wallet keys — an
/// attacker holding a user's key drains $100/day max, not the full
/// balance instantly.
///
/// Paused: rejected with VaultPaused while the vault is paused.
#[derive(Accounts)]
pub struct Withdraw<'info> {
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

pub fn handle_withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
    // v0.15.0: emergency pause guard.
    require!(!ctx.accounts.vault_state.paused, VaultError::VaultPaused);

    let user = &mut ctx.accounts.user_account;

    // Sufficient balance check.
    require!(user.balance >= amount, VaultError::InsufficientBalance);

    // v0.15.0: rolling 24h cap. Reset the window if 24h has elapsed since
    // it started (or if this is the user's first-ever withdraw, since
    // daily_window_start defaults to 0 → "lots of time elapsed" → reset).
    let now = Clock::get()?.unix_timestamp;
    let elapsed = now.saturating_sub(user.daily_window_start);
    if elapsed >= DAILY_WINDOW_SECONDS {
        user.daily_window_start = now;
        user.daily_withdrawn = 0;
    }
    let new_daily = user
        .daily_withdrawn
        .checked_add(amount)
        .ok_or(VaultError::Overflow)?;
    require!(
        new_daily <= DAILY_WITHDRAW_CAP,
        VaultError::WithdrawDailyCapExceeded
    );
    user.daily_withdrawn = new_daily;

    // Debit balance + accumulate lifetime stat.
    user.balance = user.balance.checked_sub(amount).ok_or(VaultError::Overflow)?;
    user.total_withdrawn = user
        .total_withdrawn
        .checked_add(amount)
        .ok_or(VaultError::Overflow)?;

    // Transfer tokens from vault → user. The vault_state PDA signs the
    // CPI (it's the authority on the vault token account).
    let seeds = &[b"vault".as_ref(), &[ctx.accounts.vault_state.bump]];
    let signer_seeds = &[&seeds[..]];

    let cpi_accounts = Transfer {
        from: ctx.accounts.vault_token_account.to_account_info(),
        to: ctx.accounts.user_token_account.to_account_info(),
        authority: ctx.accounts.vault_state.to_account_info(),
    };
    let cpi_ctx = CpiContext::new_with_signer(
        ctx.accounts.token_program.to_account_info(),
        cpi_accounts,
        signer_seeds,
    );
    token::transfer(cpi_ctx, amount)?;

    msg!(
        "Withdrew {} from user {} (daily window: {}/{} used)",
        amount,
        user.wallet,
        user.daily_withdrawn,
        DAILY_WITHDRAW_CAP
    );
    Ok(())
}
