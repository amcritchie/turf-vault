use anchor_lang::prelude::*;
use anchor_spl::token::{self, Mint, Token, TokenAccount, Transfer};
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus};
use crate::errors::VaultError;

/// `settle_contest` — grade a contest, disburse USDC payouts.
///
/// For each Settlement entry, performs an SPL transfer of `payout` USDC
/// from the contest's prize_pool PDA → the winner's USDC ATA (signed by
/// vault_state PDA), then updates the ContestEntry and UserAccount stats.
///
/// remaining_accounts pattern: per settlement, 3 accounts in order:
///   [user_account_pda, contest_entry_pda, winner_usdc_ata]
///
/// Validation:
///   - 2-of-3 multisig (constraint).
///   - contest.status is Open or Locked.
///   - payout_mint == vault_state.payout_mint (USDC pin).
///   - sum(payouts) <= contest.prize_pool. Entry fees are NOT included
///     anymore — they sit in op_rev ATAs, separate from the prize pool.
///   - No duplicate (wallet, entry_num) in the vec.
///   - remaining_accounts.len() == settlements.len() * 3.
///
/// Per winner:
///   - SPL Transfer (PDA-signed): prize_pool → winner_usdc_ata.
///   - ContestEntry: status Active → Won (if payout > 0) or Lost.
///   - UserAccount: total_won += payout, cashes += (payout > 0),
///     wins += (rank == 1).
///
/// Final: contest.status = Settled.
///
/// CU budget: roughly 5k base + 7k per winner; ≤ 25 winners fits the
/// default 200k budget. Rails sets `set_compute_unit_limit(400_000)`
/// on every settle TX (decided §11 Q7).
///
/// VaultState is zero-copy (v0.16). Vault is read via `load()?` for
/// signer-seed derivation; the multisig check stays in the constraint.
#[derive(AnchorSerialize, AnchorDeserialize, Clone)]
pub struct Settlement {
    pub wallet: Pubkey,
    pub entry_num: u32,
    pub rank: u32,
    pub payout: u64,
}

#[derive(Accounts)]
pub struct SettleContest<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub cosigner: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"contest", contest.contest_id.as_ref()],
        bump = contest.bump,
        constraint = (contest.status == ContestStatus::Open || contest.status == ContestStatus::Locked)
            @ VaultError::ContestAlreadySettled,
    )]
    pub contest: Account<'info, Contest>,

    /// Contest's USDC prize pool PDA — source of payouts.
    #[account(
        mut,
        seeds = [b"prize_pool", contest.contest_id.as_ref()],
        bump,
        token::mint = payout_mint,
        token::authority = vault_state,
    )]
    pub prize_pool: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = payout_mint.key() == vault_state.load()?.payout_mint @ VaultError::InvalidMint,
    )]
    pub payout_mint: Box<Account<'info, Mint>>,

    pub token_program: Program<'info, Token>,

    // remaining_accounts: triples per winner.
    //   [user_account, contest_entry, winner_usdc_ata]
}

pub fn handle_settle_contest<'info>(
    ctx: Context<'_, '_, '_, 'info, SettleContest<'info>>,
    settlements: Vec<Settlement>,
) -> Result<()> {
    let contest = &mut ctx.accounts.contest;

    // Validate total payouts ≤ prize_pool (entry fees no longer count).
    let total_payouts: u64 = settlements
        .iter()
        .map(|s| s.payout)
        .try_fold(0u64, |acc, p| acc.checked_add(p))
        .ok_or(VaultError::Overflow)?;
    require!(
        total_payouts <= contest.prize_pool,
        VaultError::SettlementOverflow
    );

    // Reject duplicate (wallet, entry_num) pairs.
    let mut seen: Vec<(Pubkey, u32)> = Vec::with_capacity(settlements.len());
    for s in settlements.iter() {
        let key = (s.wallet, s.entry_num);
        require!(!seen.contains(&key), VaultError::DuplicateEntry);
        seen.push(key);
    }

    let remaining = &ctx.remaining_accounts;
    require!(
        remaining.len() == settlements.len() * 3,
        VaultError::Unauthorized
    );

    // Precompute the vault_state PDA signer seeds for SPL transfers.
    // load() returns a Ref; pull bump out so we don't hold the Ref across
    // CPI calls (the AccountInfo borrow conflicts otherwise).
    let vault_bump = ctx.accounts.vault_state.load()?.bump;
    let vault_seeds: &[&[u8]] = &[b"vault", core::slice::from_ref(&vault_bump)];
    let signer_seeds = &[vault_seeds];

    for (i, settlement) in settlements.iter().enumerate() {
        let user_account_info = &remaining[i * 3];
        let entry_account_info = &remaining[i * 3 + 1];
        let winner_ata_info = &remaining[i * 3 + 2];

        // Verify PDA seeds for user account.
        let (expected_user_pda, _) = Pubkey::find_program_address(
            &[b"user", settlement.wallet.as_ref()],
            ctx.program_id,
        );
        require!(
            user_account_info.key() == expected_user_pda,
            VaultError::Unauthorized
        );

        // Verify PDA seeds for entry.
        let (expected_entry_pda, _) = Pubkey::find_program_address(
            &[
                b"entry",
                contest.contest_id.as_ref(),
                settlement.wallet.as_ref(),
                &settlement.entry_num.to_le_bytes(),
            ],
            ctx.program_id,
        );
        require!(
            entry_account_info.key() == expected_entry_pda,
            VaultError::Unauthorized
        );

        // SPL transfer (PDA-signed): prize_pool → winner_usdc_ata.
        // Only emit the CPI if there's a non-zero payout — saves CU for
        // ranked-but-zero-payout entries.
        if settlement.payout > 0 {
            let cpi_accounts = Transfer {
                from: ctx.accounts.prize_pool.to_account_info(),
                to: winner_ata_info.clone(),
                authority: ctx.accounts.vault_state.to_account_info(),
            };
            let cpi_ctx = CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                cpi_accounts,
                signer_seeds,
            );
            token::transfer(cpi_ctx, settlement.payout)?;
        }

        // Update user account: total_won, cashes (any payout), wins (rank == 1).
        {
            let mut user_data = user_account_info.try_borrow_mut_data()?;
            let mut user: UserAccount = UserAccount::try_deserialize(&mut &user_data[..])?;
            user.total_won = user
                .total_won
                .checked_add(settlement.payout)
                .ok_or(VaultError::Overflow)?;
            if settlement.payout > 0 {
                user.cashes = user.cashes.checked_add(1).ok_or(VaultError::Overflow)?;
            }
            if settlement.rank == 1 {
                user.wins = user.wins.checked_add(1).ok_or(VaultError::Overflow)?;
            }
            let mut writer = &mut user_data[..];
            user.try_serialize(&mut writer)?;
        }

        // Update entry.
        {
            let mut entry_data = entry_account_info.try_borrow_mut_data()?;
            let mut entry: ContestEntry =
                ContestEntry::try_deserialize(&mut &entry_data[..])?;
            // Refuse to re-settle an entry already in Won/Lost. Defense in
            // depth against replay across multiple settle calls.
            require!(
                entry.status == EntryStatus::Active,
                VaultError::ContestAlreadySettled
            );
            entry.rank = settlement.rank;
            entry.payout = settlement.payout;
            entry.status = if settlement.payout > 0 {
                EntryStatus::Won
            } else {
                EntryStatus::Lost
            };
            let mut writer = &mut entry_data[..];
            entry.try_serialize(&mut writer)?;
        }
    }

    contest.status = ContestStatus::Settled;
    msg!(
        "Contest settled. {} entries processed, total_payouts: {}",
        settlements.len(),
        total_payouts
    );
    Ok(())
}
