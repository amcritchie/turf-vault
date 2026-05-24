use anchor_lang::prelude::*;
use anchor_lang::system_program;
use crate::state::{VaultState, UserAccount};
use crate::errors::VaultError;

/// `migrate_user_account` — resize a UserAccount PDA from an older layout
/// to the current v0.15.0 layout, preserving existing field values.
///
/// Handles two source layouts:
///   v0.13 (81 bytes):  [disc:8][wallet:32][balance:8][deposited:8]
///                      [withdrawn:8][won:8][seeds:8][bump:1]
///   v0.14 (113 bytes): [..v0.13 fields..][username:32][bump:1]
///                      (bump position moved from 80 → 112)
///   v0.15 (129 bytes): [..v0.14 fields..][daily_withdrawn:8]
///                      [daily_window_start:8][bump:1]
///                      (bump position moved from 112 → 128)
///
/// Uses UncheckedAccount because old-layout data can't deserialize into
/// the current UserAccount struct. Reads field-by-field from raw bytes
/// at known offsets, then resizes the account and writes the new
/// struct back. Admin pays any additional rent.
///
/// Idempotent: returns Ok with no-op if the account is already at the
/// current size.
///
/// Auth: 1-of-3 vault signer.
#[derive(Accounts)]
pub struct MigrateUserAccount<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump,
        constraint = vault_state.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    /// CHECK: UserAccount PDA with potentially old layout that can't deserialize.
    /// Verified via PDA seeds constraint + discriminator check in handler.
    #[account(
        mut,
        seeds = [b"user", wallet.key().as_ref()],
        bump,
    )]
    pub user_account: UncheckedAccount<'info>,

    /// CHECK: The wallet pubkey used as PDA seed. Not modified.
    pub wallet: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}

pub fn handle_migrate_user_account(ctx: Context<MigrateUserAccount>) -> Result<()> {
    let user_account = &ctx.accounts.user_account;
    let expected_len = 8 + UserAccount::INIT_SPACE;
    let current_len = user_account.data_len();

    // Idempotent: already at current size, no-op.
    if current_len == expected_len {
        msg!("Account already at v0.15 size, no migration needed");
        return Ok(());
    }

    // Larger than expected — refuse (shouldn't happen unless someone hand-resized).
    require!(current_len < expected_len, VaultError::AccountAlreadyMigrated);

    // Read field-by-field from raw bytes. The first eight fields (through
    // `seeds`) are at the same offsets in v0.13 and v0.14. After that
    // the layouts diverge: v0.13 has bump at 80; v0.14 has username at
    // 80..112 + bump at 112.
    let (wallet, balance, total_deposited, total_withdrawn, total_won, seeds, username, bump) = {
        let data = user_account.try_borrow_data()?;
        // v0.13 minimum size = 81 bytes (8 disc + 73 data).
        require!(data.len() >= 81, VaultError::InvalidAccountData);

        // Verify discriminator matches UserAccount. Stops a malicious caller
        // from passing a different account at the same PDA address (shouldn't
        // be possible — PDA is deterministic — but defense in depth).
        let expected_disc = UserAccount::DISCRIMINATOR;
        require!(&data[..8] == expected_disc, VaultError::InvalidAccountData);

        // Shared v0.13 + v0.14 prefix.
        let wallet = Pubkey::try_from(&data[8..40])
            .map_err(|_| error!(VaultError::InvalidAccountData))?;
        let balance = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let total_deposited = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let total_withdrawn = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let total_won = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let seeds = u64::from_le_bytes(data[72..80].try_into().unwrap());

        // Detect v0.13 vs v0.14 by total size.
        let (username, bump) = if data.len() >= 113 {
            // v0.14 layout: username at 80..112, bump at 112.
            let mut un = [0u8; 32];
            un.copy_from_slice(&data[80..112]);
            (un, data[112])
        } else {
            // v0.13 layout: no username, bump at 80.
            ([0u8; 32], data[80])
        };

        (wallet, balance, total_deposited, total_withdrawn, total_won, seeds, username, bump)
    }; // data borrow dropped here

    // Realloc to v0.15 size (129 bytes total = 8 disc + 121 data).
    user_account.to_account_info().resize(expected_len)?;

    // Top up rent if the new size needs more lamports.
    let rent = Rent::get()?;
    let new_min = rent.minimum_balance(expected_len);
    let current_lamports = user_account.lamports();
    if new_min > current_lamports {
        let diff = new_min - current_lamports;
        system_program::transfer(
            CpiContext::new(
                ctx.accounts.system_program.to_account_info(),
                system_program::Transfer {
                    from: ctx.accounts.admin.to_account_info(),
                    to: user_account.to_account_info(),
                },
            ),
            diff,
        )?;
    }

    // Write the v0.15 layout. New fields (daily_withdrawn,
    // daily_window_start) initialize to zero — the user's first withdraw
    // post-migration will see >24h elapsed since the epoch and reset the
    // window cleanly.
    let new_account = UserAccount {
        wallet,
        balance,
        total_deposited,
        total_withdrawn,
        total_won,
        seeds,
        username,
        daily_withdrawn: 0,
        daily_window_start: 0,
        bump,
    };

    let mut data = user_account.try_borrow_mut_data()?;
    let mut cursor = &mut data[8..]; // skip discriminator (already correct)
    new_account.serialize(&mut cursor)?;

    msg!(
        "User account migrated for: {} ({} -> {} bytes)",
        wallet,
        current_len,
        expected_len
    );
    Ok(())
}
