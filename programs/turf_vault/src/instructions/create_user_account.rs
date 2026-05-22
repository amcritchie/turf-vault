use anchor_lang::prelude::*;
use crate::state::UserAccount;

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
    user.bump = ctx.bumps.user_account;

    msg!("User account created for: {}", wallet);
    Ok(())
}
