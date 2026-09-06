use anchor_lang::prelude::*;
use solana_program::hash::hash;
use crate::state::{VaultState, EntryTokenAccount, entry_token_source};
use crate::errors::VaultError;

/// `burn_entry_token` — admin voids an unspent contest-entry voucher.
///
/// The counterpart to `mint_entry_token`: an operator claw-back for a token
/// granted in error, granted to a fraudulent account, or granted against a
/// purchase later refunded or charged back. The token is destroyed as SPENDING
/// POWER — after this it can never fund an entry — but the account itself
/// SURVIVES as a tombstone.
///
/// ── Why a tombstone and not `close = admin` ───────────────────────────────
///
/// Rails derives what a user is OWED from the on-chain token count, not from a
/// database mirror:
///
///     owed = (seeds / SEEDS_PER_LEVEL) - tokens.length
///     (admin/free_entries_controller.rb, and Tokens::LevelUpGrant#missing_levels)
///
/// Closing the PDA would drop `tokens.length`, so a burned token would
/// immediately re-read as owed — the admin page's own "Mint all" button, and the
/// next level-up sweep, would mint it straight back. The burn would undo itself.
/// Keeping the account keeps the count, so nothing re-mints. The rent stays
/// locked in the tombstone rather than being refunded to the admin; that is the
/// price of a burn that sticks.
///
/// ── Why no new `burned` field ─────────────────────────────────────────────
///
/// `EntryTokenAccount::LEN` is 124 bytes and FULLY PACKED — there are no
/// reserved bytes. Adding `burned: bool` + `burned_at: Option<i64>` grows the
/// struct by 10, and the moment it grows, `Account<'info, EntryTokenAccount>`
/// fails to deserialize every account already on chain at the old size. That
/// would break `enter_contest_with_token` for EVERY token minted before this
/// upgrade. `realloc` does not rescue it either: Anchor deserializes the account
/// BEFORE the realloc constraint runs, so the read fails first.
///
/// So the tombstone is recorded INSIDE the existing layout, at zero size cost:
///
///   * `consumed = true` is what actually blocks the spend. It reuses the
///     constraint `enter_contest_with_token` already carries
///     (`!entry_token.consumed`), so no other instruction changes and no
///     already-minted account is put at risk.
///   * `source |= BURNED_FLAG` (bit 7) is what distinguishes a BURN from a
///     genuine SPEND. `source` holds small enum values (0..=5) and
///     `mint_entry_token` assigns it raw with no range check, so the high bit is
///     free. Provenance survives in the low 7 bits — a burned Stripe token still
///     reads as a Stripe token — which keeps the audit trail intact.
///
/// A reader wanting provenance masks with `SOURCE_MASK`; a reader asking "burned
/// or spent?" tests `BURNED_FLAG`.
///
/// ── Auth: 1-of-3 vault signer; the owner does NOT sign ────────────────────
///
/// Matches `mint_entry_token`. This is deliberately the OPPOSITE of OPSEC-004's
/// ruling on `enter_contest_with_token`, and the asymmetry is sound: there, an
/// admin-only consume could SPEND a user's token on a contest of the admin's
/// choosing, silently converting the user's property into an entry they never
/// picked. Here the token is destroyed and no entry is created, so a rogue
/// signer can vandalize but cannot misappropriate — and that same signer could
/// simply have declined to mint in the first place. Requiring the owner's
/// signature would defeat the feature outright: a claw-back is aimed exactly at
/// the holder who will not surrender the token.
///
/// NOT gated by vault pause — a pause is when an operator most needs to stop bad
/// vouchers from being redeemed. Mirrors `mint_entry_token`, likewise ungated.
///
/// VaultState is zero-copy (v0.16) — `load()?` for the signer check.
#[derive(Accounts)]
#[instruction(source_ref_hash: [u8; 32])]
pub struct BurnEntryToken<'info> {
    /// Vault signer (admin). NOT `mut`: a tombstone refunds no rent and creates
    /// no account, so this instruction moves no lamports beyond the tx fee.
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    /// The voucher being voided.
    ///
    /// SEED-BOUND TO `source_ref_hash` ON PURPOSE — this is the fat-finger guard.
    /// A burn is destructive and irreversible, so the caller must name its target
    /// TWICE: once by passing the account, once by passing the ref hash it should
    /// derive from. Naming the wrong account fails the seeds check instead of
    /// quietly burning some other user's token. (The hash is an instruction arg
    /// rather than a `hash()` call inside the seed expression because `anchor idl
    /// build` cannot represent a function-call seed — the same reason, and the
    /// same shape, as `mint_entry_token`. The handler's assert below closes the
    /// loop by binding that arg back to the stored `source_ref`.)
    ///
    /// Both constraints are load-bearing and neither implies the other:
    ///   * the `BURNED_FLAG` check rejects a DOUBLE burn. It cannot be folded
    ///     into `!consumed`, because a burn sets `consumed = true` itself;
    ///     without this guard a re-burn would be accepted and would overwrite
    ///     `consumed_at`, destroying the record of when the burn happened.
    ///   * `!consumed` rejects a token already SPENT on a real entry — burning
    ///     one would rewrite the history of an entry that exists and stands.
    ///
    /// THE ORDER IS THE CONTRACT, not style. Anchor evaluates constraints top to
    /// bottom and returns the FIRST failure, and a burn sets `consumed` — so
    /// with `!consumed` written first, a double burn tripped THAT check and
    /// reported `EntryTokenAlreadyConsumed`, which reads as "already spent on an
    /// entry". `EntryTokenAlreadyBurned` was unreachable: dead code behind a
    /// misleading message, on the one action an operator cannot undo. Checking
    /// the flag FIRST is what makes each error mean what it says — burned tokens
    /// answer "already burned", spent tokens answer "already consumed". Caught by
    /// the local-validator run, not by reading.
    #[account(
        mut,
        seeds = [b"entry_token".as_ref(), source_ref_hash.as_ref()],
        bump = entry_token.bump,
        constraint = entry_token.source & entry_token_source::BURNED_FLAG == 0
            @ VaultError::EntryTokenAlreadyBurned,
        constraint = !entry_token.consumed @ VaultError::EntryTokenAlreadyConsumed,
    )]
    pub entry_token: Account<'info, EntryTokenAccount>,
}

pub fn handle_burn_entry_token(
    ctx: Context<BurnEntryToken>,
    source_ref_hash: [u8; 32],
) -> Result<()> {
    // Bind the seed arg back to the account's OWN stored ref. Without this the
    // seeds check proves only "this address derives from the hash you supplied",
    // which a caller supplying a matched (wrong-account, wrong-hash) pair
    // satisfies trivially. With it, the burn is provably aimed at the token whose
    // stored source_ref really is the one named. Mirrors mint_entry_token's
    // audit #9 assert.
    require!(
        hash(&ctx.accounts.entry_token.source_ref).to_bytes() == source_ref_hash,
        VaultError::EntryTokenSeedMismatch
    );

    let now = Clock::get()?.unix_timestamp;
    let entry_token = &mut ctx.accounts.entry_token;

    entry_token.consumed = true;
    entry_token.consumed_at = Some(now);
    entry_token.source |= entry_token_source::BURNED_FLAG;

    msg!(
        "Entry token burned for {} (source: {}, burned at {})",
        entry_token.owner,
        entry_token.source & entry_token_source::SOURCE_MASK,
        now
    );
    Ok(())
}
