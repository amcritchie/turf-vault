use anchor_lang::prelude::*;
use crate::state::{VaultState, EntryTokenAccount};
use crate::errors::VaultError;

/// `mint_entry_token` — admin mints a pre-purchased contest-entry voucher
/// for a user.
///
/// The recipient (user) later consumes one of these via
/// enter_contest_with_token to enter a contest without paying the USDC
/// entry fee at entry time. Typical triggers: a Stripe purchase
/// (TokenPurchaseJob), an operator gift (admin UI), a level-up reward.
///
/// PDA seeds: [b"entry_token", user_wallet, sequence_le_bytes]. `sequence`
/// is supplied by the caller (Rails picks `current_count` for the user)
/// so multiple tokens can coexist for the same wallet without PDA
/// collisions. Discover with `getProgramAccounts` filtered by `owner`.
///
/// Auth: 1-of-3 vault signer (routine op). The user_wallet is NOT a
/// signer — tokens can be minted for any wallet. (Consent at redemption
/// time is enforced separately by the consume instructions.)
///
/// NOT gated by vault pause — operators must be able to fulfill Stripe
/// purchases that completed before the pause.
///
/// VaultState is zero-copy (v0.16) — load()? for the signer check.
#[derive(Accounts)]
#[instruction(sequence: u64)]
pub struct MintEntryToken<'info> {
    /// Vault signer (admin) — pays SOL rent for the EntryTokenAccount PDA.
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

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
