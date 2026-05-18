use anchor_lang::prelude::*;
use crate::state::{VaultState, EntryTokenAccount};
use crate::errors::VaultError;

/// Admin mints an EntryTokenAccount for a user. The token represents one
/// pre-purchased contest entry that the user can later redeem (consume) to
/// enter a contest without paying the entry fee at entry time.
///
/// PDA seeds: [b"entry_token", user_wallet, sequence_le_bytes]
/// `sequence` is supplied by the caller so admin can mint multiple tokens per
/// user without PDA collisions. Discovery is via getProgramAccounts filtered
/// by `owner`.
///
/// Auth: any 1-of-3 vault signer (same routine-op pattern as create_contest,
/// enter_contest, migrate_user_account). The user_wallet is NOT a signer.
#[derive(Accounts)]
#[instruction(sequence: u64)]
pub struct MintEntryToken<'info> {
    /// Vault signer (admin) — pays SOL rent for the EntryTokenAccount PDA.
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.bump,
        constraint = vault_state.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: Account<'info, VaultState>,

    /// CHECK: The recipient wallet. Used only as a PDA seed and to set
    /// `owner` on the new account. Not a signer; not modified.
    pub user_wallet: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        space = EntryTokenAccount::LEN,
        seeds = [
            b"entry_token",
            user_wallet.key().as_ref(),
            &sequence.to_le_bytes(),
        ],
        bump,
    )]
    pub entry_token: Account<'info, EntryTokenAccount>,

    pub system_program: Program<'info, System>,
}

pub fn handle_mint_entry_token(
    ctx: Context<MintEntryToken>,
    _sequence: u64,
    source: u8,
    source_ref: [u8; 64],
) -> Result<()> {
    let entry_token = &mut ctx.accounts.entry_token;
    entry_token.owner = ctx.accounts.user_wallet.key();
    entry_token.source = source;
    entry_token.source_ref = source_ref;
    entry_token.consumed = false;
    entry_token.consumed_at = None;
    entry_token.created_at = Clock::get()?.unix_timestamp;
    entry_token.bump = ctx.bumps.entry_token;

    msg!(
        "Entry token minted for {} (source: {}, sequence: {})",
        entry_token.owner,
        source,
        _sequence
    );
    Ok(())
}
