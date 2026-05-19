use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus, Season};
use crate::errors::VaultError;

/// Direct entry: user transfers USDC from their own wallet ATA to the vault.
/// No UserAccount PDA balance deduction — funds come straight from the user's token account.
/// Admin is payer (covers PDA rent) so the user only spends USDC, not SOL.
#[derive(Accounts)]
#[instruction(entry_num: u32)]
pub struct EnterContestDirect<'info> {
    /// Admin pays rent for the ContestEntry PDA
    #[account(mut)]
    pub payer: Signer<'info>,

    /// The user's Phantom wallet — signs the token transfer
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
        mut,
        constraint = contest.status == ContestStatus::Open @ VaultError::ContestNotOpen,
        constraint = contest.current_entries < contest.max_entries @ VaultError::ContestFull,
    )]
    pub contest: Account<'info, Contest>,

    #[account(
        init,
        payer = payer,
        space = 8 + ContestEntry::INIT_SPACE,
        seeds = [
            b"entry",
            contest.contest_id.as_ref(),
            user.key().as_ref(),
            &entry_num.to_le_bytes(),
        ],
        bump,
    )]
    pub contest_entry: Account<'info, ContestEntry>,

    #[account(
        constraint = mint.key() == vault_state.usdc_mint @ VaultError::InvalidMint,
    )]
    pub mint: Account<'info, Mint>,

    /// User's USDC token account (ATA) — source of entry fee
    #[account(
        mut,
        token::mint = mint,
        token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    /// Vault's USDC token account — destination for entry fee
    #[account(
        mut,
        constraint = vault_token_account.key() == vault_state.vault_usdc @ VaultError::InvalidMint,
    )]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,

    /// Season whose seed_schedule controls per-entry seed awards.
    pub season: Account<'info, Season>,

    pub system_program: Program<'info, System>,
}

pub fn handle_enter_contest_direct(ctx: Context<EnterContestDirect>, entry_num: u32) -> Result<()> {
    let contest = &mut ctx.accounts.contest;

    // Transfer entry fee from user's ATA to vault's ATA
    let cpi_accounts = Transfer {
        from: ctx.accounts.user_token_account.to_account_info(),
        to: ctx.accounts.vault_token_account.to_account_info(),
        authority: ctx.accounts.user.to_account_info(),
    };
    let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
    token::transfer(cpi_ctx, contest.entry_fee)?;

    // Update contest state
    contest.entry_fees = contest.entry_fees.checked_add(contest.entry_fee).ok_or(VaultError::Overflow)?;
    contest.current_entries = contest.current_entries.checked_add(1).ok_or(VaultError::Overflow)?;

    // Award seeds from the season schedule (entries 5+ clamp to slot 4)
    let user_account = &mut ctx.accounts.user_account;
    let season = &ctx.accounts.season;
    let idx = (entry_num as usize).min(4);
    user_account.seeds = user_account.seeds.checked_add(season.seed_schedule[idx]).ok_or(VaultError::Overflow)?;

    // Create entry
    let entry = &mut ctx.accounts.contest_entry;
    entry.contest_id = contest.contest_id;
    entry.wallet = ctx.accounts.user.key();
    entry.entry_num = entry_num;
    entry.status = EntryStatus::Active;
    entry.rank = 0;
    entry.payout = 0;
    entry.bump = ctx.bumps.contest_entry;

    msg!(
        "Direct entry {} for wallet {} in contest. Fee: {}, Entry fees: {}",
        entry_num,
        entry.wallet,
        contest.entry_fee,
        contest.entry_fees
    );
    Ok(())
}
