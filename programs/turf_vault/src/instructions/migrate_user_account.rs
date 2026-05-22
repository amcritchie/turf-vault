use anchor_lang::prelude::*;
use anchor_lang::system_program;
use crate::state::{VaultState, UserAccount};
use crate::errors::VaultError;

/// Migration instruction: reallocs a UserAccount PDA to the current struct size
/// and preserves existing data. Admin-only, idempotent (no-op if already correct size).
///
/// Uses UncheckedAccount because old-layout accounts can't deserialize into the
/// current UserAccount struct (bump position shifted when seeds field was added).
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

    // Already correct size — idempotent no-op
    if current_len == expected_len {
        msg!("Account already at current size, no migration needed");
        return Ok(());
    }

    // Larger than expected — something is wrong
    require!(current_len < expected_len, VaultError::AccountAlreadyMigrated);

    // Read old fields from raw bytes at known offsets.
    // Pre-v0.14.0 layout (81 bytes): [disc:8][wallet:32][balance:8][deposited:8][withdrawn:8][won:8][seeds:8][bump:1]
    // v0.14.0 layout (113 bytes):    [..][seeds:8][username:32][bump:1]
    // The bump byte shifts position when fields are added before it, so we read field-by-field.
    let (wallet, balance, total_deposited, total_withdrawn, total_won, seeds, bump) = {
        let data = user_account.try_borrow_data()?;
        require!(data.len() >= 81, VaultError::InvalidAccountData);

        // Verify discriminator matches UserAccount
        let expected_disc = UserAccount::DISCRIMINATOR;
        require!(&data[..8] == expected_disc, VaultError::InvalidAccountData);

        let wallet = Pubkey::try_from(&data[8..40])
            .map_err(|_| error!(VaultError::InvalidAccountData))?;
        let balance = u64::from_le_bytes(data[40..48].try_into().unwrap());
        let total_deposited = u64::from_le_bytes(data[48..56].try_into().unwrap());
        let total_withdrawn = u64::from_le_bytes(data[56..64].try_into().unwrap());
        let total_won = u64::from_le_bytes(data[64..72].try_into().unwrap());
        let seeds = u64::from_le_bytes(data[72..80].try_into().unwrap());
        let bump = data[80]; // In the pre-v0.14.0 layout, bump is right after seeds

        (wallet, balance, total_deposited, total_withdrawn, total_won, seeds, bump)
    }; // data borrow dropped here

    // Realloc account to new size
    user_account.to_account_info().resize(expected_len)?;

    // Pay rent difference from admin
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

    // Write new struct with an empty username for the new field —
    // the Rails app backfills the real value via set_username.
    let new_account = UserAccount {
        wallet,
        balance,
        total_deposited,
        total_withdrawn,
        total_won,
        seeds,
        username: [0u8; 32],
        bump,
    };

    let mut data = user_account.try_borrow_mut_data()?;
    let mut cursor = &mut data[8..]; // skip discriminator (already correct)
    new_account.serialize(&mut cursor)?;

    msg!("User account migrated for: {} ({} -> {} bytes)", wallet, current_len, expected_len);
    Ok(())
}
