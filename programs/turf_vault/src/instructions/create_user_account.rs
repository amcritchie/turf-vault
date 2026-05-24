use anchor_lang::prelude::*;
use crate::state::UserAccount;

/// `create_user_account` — first-touch onboarding for a wallet.
///
/// Allocates a UserAccount PDA at the v0.15.0 layout. All counters start
/// at zero. The username is set at creation time (per-byte zero-padded);
/// owner can change it later via `set_username`.
///
/// Permissionless payer: anyone can pay SOL rent for any wallet's account
/// (enables operator-funded onboarding). The wallet itself is NOT a
/// signer on this call — Rails creates it after a managed-wallet
/// generates, or before the first Phantom-wallet entry.
#[derive(Accounts)]
#[instruction(wallet: Pubkey)]
pub struct CreateUserAccount<'info> {
    #[account(mut)]
    pub payer: Signer<'info>,

    #[account(
        init,
        payer = payer,
        space = 8 + UserAccount::INIT_SPACE,
        seeds = [b"user", wallet.as_ref()],
        bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    pub system_program: Program<'info, System>,
}

pub fn handle_create_user_account(
    ctx: Context<CreateUserAccount>,
    wallet: Pubkey,
    username: [u8; 32],
) -> Result<()> {
    let user = &mut ctx.accounts.user_account;
    user.wallet = wallet;
    user.balance = 0;
    user.total_deposited = 0;
    user.total_withdrawn = 0;
    user.total_won = 0;
    user.seeds = 0;
    user.username = username;
    // v0.15.0 daily-cap fields. daily_window_start = 0 means "no window
    // yet" — the first withdraw will see >24h elapsed since the epoch
    // and initialize the window correctly.
    user.daily_withdrawn = 0;
    user.daily_window_start = 0;
    user.bump = ctx.bumps.user_account;

    msg!("User account created for: {}", wallet);
    Ok(())
}
