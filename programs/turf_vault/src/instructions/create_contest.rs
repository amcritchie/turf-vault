use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, Contest, ContestStatus};
use crate::errors::VaultError;

/// `create_contest` — set up a new contest with an upfront prize pool.
///
/// Dual-signer pattern:
///   - `payer` (any 1-of-3 vault signer): pays SOL rent for the Contest PDA
///   - `creator` (any wallet with USDC): signs the SPL transfer of the
///     `prizes` amount from their ATA → vault USDC PDA
///
/// For operator-funded contests, admin acts as BOTH payer and creator (Solana
/// dedupes signers by pubkey so a single signature covers both slots — see
/// `Solana::Vault#create_contest_server_funded`).
///
/// Validation:
///   - `payout_amounts.sum() == prizes` (no slop)
///   - sum uses checked_add (OPSEC-025 — `iter().sum::<u64>()` wraps silently)
///   - vault_token_account must be the canonical vault_usdc PDA
///   - mint must be USDC
///
/// `season_id` binds the contest to one Season for seed-schedule pinning
/// (OPSEC-023).
#[derive(Accounts)]
#[instruction(contest_id: [u8; 32])]
pub struct CreateContest<'info> {
    /// Admin bot — pays SOL rent for Contest PDA
    #[account(mut)]
    pub payer: Signer<'info>,

    /// Creator's Phantom wallet — signs USDC transfer for prizes
    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.is_signer(&payer.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        init,
        payer = payer,
        space = 8 + Contest::INIT_SPACE,
        seeds = [b"contest", contest_id.as_ref()],
        bump,
    )]
    pub contest: Account<'info, Contest>,

    #[account(constraint = mint.key() == vault_state.usdc_mint @ VaultError::InvalidMint)]
    pub mint: Account<'info, Mint>,

    /// Creator's USDC token account (ATA) — source of prizes
    #[account(mut, token::mint = mint, token::authority = creator)]
    pub creator_token_account: Account<'info, TokenAccount>,

    /// Vault's USDC token account — destination for prizes
    #[account(mut, constraint = vault_token_account.key() == vault_state.vault_usdc @ VaultError::InvalidMint)]
    pub vault_token_account: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}

pub fn handle_create_contest(
    ctx: Context<CreateContest>,
    contest_id: [u8; 32],
    season_id: u32,
    entry_fee: u64,
    max_entries: u32,
    payout_amounts: Vec<u64>,
    prizes: u64,
) -> Result<()> {
    // Validate payout_amounts sum == prizes.
    // OPSEC-025: iter().sum::<u64>() wraps silently on overflow in release
    // builds — a crafted payout_amounts like [u64::MAX, 1] would sum to 0 and
    // pass an equality check against prizes=0. Use checked addition.
    let total_payouts: u64 = payout_amounts
        .iter()
        .try_fold(0u64, |acc, &x| acc.checked_add(x))
        .ok_or(VaultError::Overflow)?;
    require!(total_payouts == prizes, VaultError::InvalidPayoutTiers);

    let contest = &mut ctx.accounts.contest;
    contest.contest_id = contest_id;
    contest.prizes = prizes;
    contest.entry_fee = entry_fee;
    contest.entry_fees = 0;
    contest.max_entries = max_entries;
    contest.current_entries = 0;
    contest.status = ContestStatus::Open;
    contest.payout_amounts = payout_amounts;
    contest.admin = ctx.accounts.payer.key();
    contest.creator = ctx.accounts.creator.key();
    contest.season_id = season_id;
    contest.bump = ctx.bumps.contest;

    // Transfer prizes USDC from creator's ATA → vault
    if prizes > 0 {
        let cpi_accounts = Transfer {
            from: ctx.accounts.creator_token_account.to_account_info(),
            to: ctx.accounts.vault_token_account.to_account_info(),
            authority: ctx.accounts.creator.to_account_info(),
        };
        let cpi_ctx = CpiContext::new(ctx.accounts.token_program.to_account_info(), cpi_accounts);
        token::transfer(cpi_ctx, prizes)?;
    }

    msg!("Contest created with fee: {}, prizes: {}", entry_fee, prizes);
    Ok(())
}
