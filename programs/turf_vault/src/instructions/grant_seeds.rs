use anchor_lang::prelude::*;

use crate::errors::VaultError;
use crate::state::{seed_grant_kind, SeedGrant, UserAccount, VaultState, MAX_GRANT_SEEDS};

/// Admin-signed standalone seed grant (Rails "quest" bonuses). 1-of-3 vault signer.
///
/// Credits a fixed `amount` of loyalty seeds into a user's UserAccount PDA
/// OUTSIDE the normal enter_contest flow — used for: first manual username
/// change, newsletter join, and a friend-invite-entered reward.
///
/// Idempotency is on-chain: the `[b"seed_grant", user_wallet, kind, invitee]`
/// guard PDA is `init`-once, so a repeat grant of the same (user, kind[, invitee])
/// collides on init and the TX fails — true double-grant protection, safe for
/// Sidekiq retries (mirrors the entry_token init-as-guard, v0.19 #9).
///   - USERNAME_FIRST_CHANGE / NEWSLETTER_JOIN: invitee = Pubkey::default()
///     → one grant per (user, kind) EVER.
///   - INVITE_FRIEND: invitee = the friend's wallet → one grant per
///     (inviter, invitee) — fires once per distinct invited friend.
///
/// The admin credits the user WITHOUT the user signing — consistent with
/// settle_contest mutating arbitrary UserAccounts under admin authority.
/// `amount` is bounded by MAX_GRANT_SEEDS so a leaked 1-of-3 key can't mint
/// unbounded seeds in a single call.
#[derive(Accounts)]
#[instruction(amount: u64, kind: u8, invitee: Pubkey)]
pub struct GrantSeeds<'info> {
    /// Vault signer (1-of-3). Pays SOL rent for the SeedGrant guard PDA.
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        seeds = [b"vault"],
        bump = vault_state.load()?.bump,
        constraint = vault_state.load()?.is_signer(&admin.key()) @ VaultError::Unauthorized,
    )]
    pub vault_state: AccountLoader<'info, VaultState>,

    /// CHECK: recipient wallet. Not a signer — the admin credits seeds on the
    /// user's behalf (like settle_contest). Used to derive + bind the
    /// UserAccount PDA and to seed the guard PDA.
    pub user_wallet: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"user", user_wallet.key().as_ref()],
        bump = user_account.bump,
        constraint = user_account.wallet == user_wallet.key() @ VaultError::Unauthorized,
    )]
    pub user_account: Account<'info, UserAccount>,

    /// Idempotency guard — `init`-once per (user, kind[, invitee]).
    #[account(
        init,
        payer = admin,
        space = 8 + SeedGrant::INIT_SPACE,
        seeds = [b"seed_grant".as_ref(), user_wallet.key().as_ref(), &[kind], invitee.as_ref()],
        bump,
    )]
    pub seed_grant: Account<'info, SeedGrant>,

    pub system_program: Program<'info, System>,
}

pub fn handle_grant_seeds(
    ctx: Context<GrantSeeds>,
    amount: u64,
    kind: u8,
    invitee: Pubkey,
) -> Result<()> {
    // Flexible kinds (v0.23): any 0..=MAX_SEED_GRANT_KIND is a valid quest, so a
    // new quest needs only a Rails kind constant — never another redeploy. The
    // per-kind guard PDA keeps each quest's once-ever lock distinct; the bound
    // still rejects a typo'd kind from opening a shadow namespace.
    require!(
        kind <= seed_grant_kind::MAX_SEED_GRANT_KIND,
        VaultError::InvalidSeedGrantKind
    );

    // INVITE_FRIEND carries a real invitee (per-friend guard); the once-ever
    // kinds must NOT, so they can't be farmed by varying the invitee.
    if kind == seed_grant_kind::INVITE_FRIEND {
        require!(
            invitee != Pubkey::default(),
            VaultError::InvalidSeedGrantInvitee
        );
    } else {
        require!(
            invitee == Pubkey::default(),
            VaultError::InvalidSeedGrantInvitee
        );
    }

    // Bound the per-call magnitude (leaked-key blast radius + overflow hygiene).
    require!(
        amount > 0 && amount <= MAX_GRANT_SEEDS,
        VaultError::SeedGrantAmountInvalid
    );

    let user_account = &mut ctx.accounts.user_account;
    user_account.seeds = user_account
        .seeds
        .checked_add(amount)
        .ok_or(VaultError::Overflow)?;

    let grant = &mut ctx.accounts.seed_grant;
    grant.user = ctx.accounts.user_wallet.key();
    grant.kind = kind;
    grant.invitee = invitee;
    grant.amount = amount;
    grant.granted_at = Clock::get()?.unix_timestamp;
    grant.bump = ctx.bumps.seed_grant;

    msg!(
        "grant_seeds: +{} seeds to {} (kind {}, invitee {}) -> total {}",
        amount,
        grant.user,
        kind,
        invitee,
        user_account.seeds
    );

    Ok(())
}
