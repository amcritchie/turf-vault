use anchor_lang::prelude::*;
use crate::state::{VaultState, UserAccount, Contest, ContestEntry, ContestStatus, EntryStatus};
use crate::errors::VaultError;

/// `settle_contest` — grade a contest and credit payouts.
///
/// Takes a `Vec<Settlement>` (one per entry, including losers with payout=0)
/// and a pair of remaining_accounts per Settlement: [user_account, contest_entry].
/// Verifies each pair against the expected PDA seeds, then:
///   - Adds `payout` to UserAccount.balance + total_won
///   - Sets ContestEntry rank + payout, transitions Active → Won/Lost
///
/// Cap: total payouts ≤ contest.entry_fees + contest.prizes. Anchor's
/// remaining_accounts pattern lets us settle a variable number of entries
/// in one TX (up to compute-budget limits — ~25-30 entries per call on mainnet).
///
/// Auth: 2-of-3 (admin + cosigner). Settle is the only path that credits
/// balances to users, so it requires the highest authorization.
///
/// Refusals:
///   - Duplicate (wallet, entry_num) in the Vec — would double-credit a user
///   - A ContestEntry already settled (status != Active) — defense in depth
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
        bump = vault_state.bump,
        constraint = vault_state.validate_multisig(&admin.key(), &cosigner.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        constraint = contest.status == ContestStatus::Open || contest.status == ContestStatus::Locked @ VaultError::ContestAlreadySettled,
    )]
    pub contest: Account<'info, Contest>,

    // Remaining accounts: pairs of [user_account, contest_entry] for each settlement
}

pub fn handle_settle_contest(ctx: Context<SettleContest>, settlements: Vec<Settlement>) -> Result<()> {
    let contest = &mut ctx.accounts.contest;

    // Validate total payouts don't exceed entry_fees + prizes
    let total_payouts: u64 = settlements
        .iter()
        .map(|s| s.payout)
        .try_fold(0u64, |acc, p| acc.checked_add(p))
        .ok_or(VaultError::Overflow)?;

    let max_payout = contest
        .entry_fees
        .checked_add(contest.prizes)
        .ok_or(VaultError::Overflow)?;
    require!(total_payouts <= max_payout, VaultError::SettlementOverflow);

    // Reject duplicate (wallet, entry_num) pairs in the settlement vec.
    // Without this, the same entry could be credited twice in a single
    // settle call — the second iteration sees the first iteration's write
    // (since we re-deserialize from account data each pass) and adds again,
    // bypassing the total_payouts cap on the actual user balance.
    let mut seen: Vec<(Pubkey, u32)> = Vec::with_capacity(settlements.len());
    for s in settlements.iter() {
        let key = (s.wallet, s.entry_num);
        require!(!seen.contains(&key), VaultError::DuplicateEntry);
        seen.push(key);
    }

    // Process each settlement via remaining accounts
    let remaining = &ctx.remaining_accounts;
    require!(remaining.len() == settlements.len() * 2, VaultError::Unauthorized);

    for (i, settlement) in settlements.iter().enumerate() {
        // Load user account
        let user_account_info = &remaining[i * 2];
        let entry_account_info = &remaining[i * 2 + 1];

        // Verify PDA seeds for user account
        let (expected_user_pda, _) = Pubkey::find_program_address(
            &[b"user", settlement.wallet.as_ref()],
            ctx.program_id,
        );
        require!(user_account_info.key() == expected_user_pda, VaultError::Unauthorized);

        // Verify PDA seeds for entry
        let (expected_entry_pda, _) = Pubkey::find_program_address(
            &[
                b"entry",
                contest.contest_id.as_ref(),
                settlement.wallet.as_ref(),
                &settlement.entry_num.to_le_bytes(),
            ],
            ctx.program_id,
        );
        require!(entry_account_info.key() == expected_entry_pda, VaultError::Unauthorized);

        // Deserialize and update user account
        let mut user_data = user_account_info.try_borrow_mut_data()?;
        let mut user: UserAccount =
            UserAccount::try_deserialize(&mut &user_data[..])?;
        user.balance = user.balance.checked_add(settlement.payout).ok_or(VaultError::Overflow)?;
        user.total_won = user.total_won.checked_add(settlement.payout).ok_or(VaultError::Overflow)?;
        let mut writer = &mut user_data[..];
        user.try_serialize(&mut writer)?;

        // Deserialize and update entry
        let mut entry_data = entry_account_info.try_borrow_mut_data()?;
        let mut entry: ContestEntry =
            ContestEntry::try_deserialize(&mut &entry_data[..])?;
        // Refuse to mutate an entry that's already been settled. Combined
        // with the dedup check above, this blocks double-payout via any
        // path (duplicate in vec, or a second settle call referencing the
        // same entry).
        require!(entry.status == EntryStatus::Active, VaultError::ContestAlreadySettled);
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

    contest.status = ContestStatus::Settled;
    msg!("Contest settled. {} entries processed", settlements.len());
    Ok(())
}
