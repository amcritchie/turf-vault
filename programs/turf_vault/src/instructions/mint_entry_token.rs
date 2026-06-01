use anchor_lang::prelude::*;
use solana_program::hash::hash;
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
/// PDA seeds (v0.19, audit #9): [b"entry_token", source_ref_hash], where
/// `source_ref_hash` is passed by the caller and the handler asserts it equals
/// `sha256(source_ref)`. (The hash is passed as a fixed-size arg rather than
/// computed in the seed expression because `anchor idl build` cannot represent
/// a function-call seed; the on-chain assert keeps the binding sound.) The PDA
/// is therefore derived from a hash of the external reference, so re-minting the
/// same `source_ref` collides on `init` and fails — TRUE on-chain idempotency
/// (replaces the old caller-supplied `sequence`, which let the same source_ref
/// mint unlimited distinct tokens). `source_ref` must therefore be GLOBALLY
/// unique across wallets (the Stripe path namespaces by session_id; operator
/// mints must use a per-mint-unique ref). `owner` is stored in the account body
/// (not a seed), so getProgramAccounts discovery by owner is unchanged.
///
/// Auth: 1-of-3 vault signer (routine op). The user_wallet is NOT a signer —
/// tokens can be minted for any wallet. (Consent at redemption time is enforced
/// separately by the consume instructions.) The seed-based idempotency closes
/// double-mint/replay regardless of the signer count.
///
/// NOT gated by vault pause — operators must be able to fulfill Stripe
/// purchases that completed before the pause.
///
/// VaultState is zero-copy (v0.16) — load()? for the signer check.
#[derive(Accounts)]
#[instruction(source: u8, source_ref: [u8; 64], source_ref_hash: [u8; 32])]
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

    /// CHECK: The recipient wallet. Used only to set `owner` on the new
    /// account (NOT a PDA seed in v0.19). Not a signer; not modified.
    pub user_wallet: UncheckedAccount<'info>,

    #[account(
        init,
        payer = admin,
        space = EntryTokenAccount::LEN,
        seeds = [
            b"entry_token".as_ref(),
            source_ref_hash.as_ref(),
        ],
        bump,
    )]
    pub entry_token: Account<'info, EntryTokenAccount>,

    pub system_program: Program<'info, System>,
}

pub fn handle_mint_entry_token(
    ctx: Context<MintEntryToken>,
    source: u8,
    source_ref: [u8; 64],
    source_ref_hash: [u8; 32],
) -> Result<()> {
    // Bind the PDA seed hash to source_ref: the seed is a caller-supplied arg
    // (IDL-build can't encode a function-call seed), so assert it really is
    // sha256(source_ref). Catches a wrong client-side hash loudly instead of
    // minting at a PDA that doesn't match the stored ref (audit #9).
    require!(
        hash(&source_ref).to_bytes() == source_ref_hash,
        VaultError::EntryTokenSeedMismatch
    );

    let entry_token = &mut ctx.accounts.entry_token;
    entry_token.owner = ctx.accounts.user_wallet.key();
    entry_token.source = source;
    entry_token.source_ref = source_ref;
    entry_token.consumed = false;
    entry_token.consumed_at = None;
    entry_token.created_at = Clock::get()?.unix_timestamp;
    entry_token.bump = ctx.bumps.entry_token;

    msg!(
        "Entry token minted for {} (source: {})",
        entry_token.owner,
        source
    );
    Ok(())
}
