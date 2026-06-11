# Changelog

All notable changes to TurfVault are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/).

## [0.25.0] - 2026-06-10

Admin-authorized username flows: a 1-of-3 vault-signer co-signature can waive
ONLY the reserved-prefix branch of the username validity bar (v0.15.1, audit
C2). Motivation: the operator's own "Turf Monster" house account (wallet
`BLSBw8fXHzZc5pbaYCKMpMSsrtXBTbWXpUPVzMrXx9oo`) needs username `turf`, which
starts with a reserved prefix, so plain `create_user_account` fails with
`UsernameReserved` 6020 on mainnet. Privilege is the vault-signer co-signature
(`vault_state.is_signer`), NOT membership of the target wallet — the house
wallet is a 4th admin that is not (and will not be) a vault signer.

**No account-layout/byte-size change, no new error codes** (reuses
`Unauthorized` 6000). The IDL DOES change (two new instructions), so
turf-monster must re-pin `EXPECTED_IDL_HASH` from the freshly-built IDL when
this ships. Rides the next mainnet upgrade window (Squads 2-of-3 — `anchor
deploy` is dead; `scripts/squad-upgrade.js`).

### Added

- **`admin_create_user_account(wallet, username)`** — same semantics as
  `create_user_account` (permissionless payer pays rent, target wallet gets
  the PDA, wallet does NOT sign) PLUS a required `admin: Signer` validated
  `vault_state.is_signer(admin.key())` (else `Unauthorized` 6000). The plain
  variant doesn't reference `vault_state`; the admin variant adds it
  read-only. Reserved-prefix check waived; charset (printable ASCII) and
  min-length (3) still enforced.
- **`admin_set_username(username)`** — same semantics as `set_username`
  (the account OWNER signs — consenting) PLUS the same required vault-signer
  `admin` co-signer and the same prefix-only waiver.

### Changed

- **`validate_username` split** (`instructions/set_username.rs`) into
  `validate_username_charset_len` + `validate_username_prefix`;
  `validate_username` now composes the two, so plain-path behavior is
  byte-for-byte identical and no validation logic is duplicated. The admin
  variants call `validate_username_charset_len` only.
- **`init_user_account` helper** extracted in
  `instructions/create_user_account.rs` — shared field initialization for the
  plain + admin create paths so the two entry points can't drift.

### Tests

- New `admin username flows (v0.25)` block in `tests/turf_vault.ts`:
  vault-signer co-sign accepted (reserved `turf-monster` / `turf` land on
  both admin paths), non-signer admin rejected with 6000 on both, reserved
  names still rejected with 6020 on both plain paths, charset/min-length
  still enforced on the admin paths, and the owner signature still required
  for `admin_set_username`. NOTE: the suite as a whole is still the v0.15.1
  suite (references retired `deposit`/`withdraw`/`balance`) and does not run
  against the current surface — pre-existing state, see CLAUDE.md "Testing".

## [0.24.0] - 2026-06-07

Quest seed economy — admin-signed quest bonuses + on-chain per-quest reward
config. One consolidated release of the grant_seeds + flexible-kind +
Season.quest_seeds work on top of v0.20. (Folds in the interim v0.22/v0.23 dev
iterations; the v0.21 on-chain Contest name/slug was dropped — parked, never
shipped — so `create_contest` is unchanged from v0.20.)

### grant_seeds — admin-signed quest seed grants
- `grant_seeds(amount, kind, invitee)` — a 1-of-3 admin mints a quest bonus
  straight onto a wallet's `UserAccount.seeds`, outside the entry flow. Idempotent
  per `(user_wallet, kind, invitee)` via an `init`-once `SeedGrant` guard PDA, so
  each bonus mints once-ever (safe for Sidekiq retries).
- `kind` accepts any `0..=MAX_SEED_GRANT_KIND` (15), so a new quest needs only a
  Rails kind constant — never another redeploy. Named kinds: 0 username-first-change,
  1 newsletter-join, 2 invite-friend, 3 chat-message. Invite (2) carries a real
  invitee (per-friend guard); the others forbid one. `amount` ∈ 1..=`MAX_GRANT_SEEDS`
  (1000). Errors: `InvalidSeedGrantKind` / `InvalidSeedGrantInvitee` / `SeedGrantAmountInvalid`.

### Season — per-quest reward amounts on-chain
- `Season` gains `quest_seeds: [u64; 16]` (indexed by `seed_grant_kind`): each
  quest's reward lives on-chain next to the entry `seed_schedule`, so the whole
  seed economy is tuned in one place. `create_season` takes it. Season grows
  101 → 229 bytes; mint a fresh Season (immutable model, no migration).

### Account layout
- Net-new `SeedGrant` PDA + `Season` grows (+`quest_seeds`). VaultState /
  UserAccount / Contest / ContestEntry / EntryTokenAccount bodies untouched.

## [0.20.0] - 2026-06-02

Re-adds a guarded `update_signers` instruction so the multisig signer set is
mutable IN PLACE again. **No account-layout/byte-size change** — VaultState,
Contest, EntryTokenAccount bodies are untouched (`update_signers` only writes
the existing `signers: [Pubkey; 3]` field; `threshold` is left at its init
value), so deployed PDAs need no re-init. The IDL DOES change (new instruction
+ the un-retired 6017 message), so turf-monster must re-pin `EXPECTED_IDL_HASH`
from the freshly-built IDL.

> **WHY / signer-set change — read LOUDLY.** v0.16 *removed* `update_signers`,
> which made a deployed program's signer set immutable. A single compromised
> signer key (the Alex Bot server key) therefore can't be rotated in place and
> forces a full program redeploy + re-init + permanent rent loss on the old
> program. v0.20 ships `update_signers` so future signer compromise is a cheap
> 2-of-3 on-chain transaction, never a redeploy. The redeploy that introduces
> v0.20 itself (forced by the current Alex Bot key leak) is documented in
> `docs/KEY_ROTATION.md`.

### Added

- **`update_signers(new_signers: [Pubkey; 3])` (2-of-3).** Rotates one or more
  signer pubkeys in place. **Threshold is PINNED at 2-of-3** — there is no
  `new_threshold` parameter. `validate_multisig` never reads the `threshold`
  field (it structurally requires two distinct signers), so a configurable
  threshold was inert and misleading; the arg was dropped. Auth is
  `validate_multisig(admin, cosigner)` — two DISTINCT current signers. Adapted
  to the v0.16+ zero-copy `#[account(zero_copy(unsafe))]` VaultState
  (`AccountLoader` + `load()` in the constraint, `load_mut()` for the write);
  the v0.15.x original predated zero-copy. Logs old→new signers via `msg!`.

### Safety (signer continuity — can't brick the multisig)

- **No duplicate signers** (reuses 6014 `DuplicateSigner`).
- **No default/zeroed slots** — a `Pubkey::default()` slot is rejected
  (`SignerContinuityRequired` 6017).
- **Continuity:** BOTH cosigners that authorized the update must remain in the
  new set (`SignerContinuityRequired` 6017). Governance is 2-of-3, so a working
  multisig needs TWO surviving cosignable keys — a "keep ≥1" guard would pass a
  rotation to `[survivor, junk, junk]` that still bricks all governance (no
  second key could ever cosign). Requiring both `admin` and `cosigner` to
  survive guarantees two known-good keys remain. The intended "evict the leaked
  bot key" rotation still passes (Alex + Mason cosign and both stay).

### Errors

- **6017 `SignerContinuityRequired` UN-RETIRED** (was reserved-but-unused in
  v0.16–v0.19, marked "no `update_signers`"). Message updated to also cover the
  default/zeroed-slot rejection. No code renumbering.

### Coordination owed (FOLLOWS the redeploy/upgrade, not before)

- solana-studio: `update_signers` transaction builder + IDL bump to 0.20.0.
- turf-monster: re-pin `EXPECTED_IDL_HASH` from the freshly-built IDL (Squad
  upgrades / fresh deploys don't update the on-chain IDL — use `target/idl`,
  NOT `anchor idl fetch`); optional admin cosign UI for a signer rotation.
- **NOT YET DEPLOYED.** Needs an adversarial mini-review before any deploy.
  The rotation redeploy is gated on that review + operator go.

## [0.19.0] - 2026-05-31

Security hardening from the 2026-05-31 adversarial audit (highs #3 / #5 / #6 / #9).
No account-layout/byte-size change — Contest, EntryTokenAccount, VaultState bodies
are untouched, so existing PDAs need no re-init. The IDL DOES change (new errors +
`mint_entry_token` args + `set_contest_lock_time`/`set_contest_conclusion_time`
optional cosigner), so turf-monster must re-pin `EXPECTED_IDL_HASH` from the
freshly-built IDL.

### Security

- **#3 `settle_contest` — bind payout destination.** Each winner's payout must
  now land in `get_associated_token_address(settlement.wallet, payout_mint)`,
  validated for every settlement row before the SPL transfer. Previously the
  destination ATA was unconstrained, so a settle authority could redirect the
  prize pool to any same-mint account while stats credited the real winner.
  New error 6036 `InvalidPayoutDestination`.
- **#6 `settle_contest` — entries-closed gate.** Settle now requires the derived
  lock OR conclusion timestamp to have passed (reuses 6028 `ContestNotLocked`),
  so a contest can't be graded while still open for entries.
- **#5 `set_contest_lock_time` / `set_contest_conclusion_time` — finality + multisig.**
  Pre-lock changes stay 1-of-3; amending a lock that has ALREADY PASSED requires
  2-of-3 (optional `cosigner` + `validate_multisig`), closing the results-known
  late-entry re-open vector. A set conclusion is likewise 2-of-3 to amend (first
  set stays 1-of-3). Timestamps must be non-negative, a set conclusion must be in
  the future, and a set lock must precede a set conclusion. New error 6037
  `InvalidTimestamp`.
- **#9 `mint_entry_token` — on-chain idempotency.** The EntryTokenAccount PDA is
  now seeded on `sha256(source_ref)` (passed as `source_ref_hash`, asserted
  on-chain) instead of a caller-supplied `sequence`, so re-minting the same
  `source_ref` collides on `init` and fails. Stays 1-of-3. `source_ref` must be
  globally unique across wallets. New error 6038 `EntryTokenSeedMismatch`.
  Account body unchanged; `owner` still drives getProgramAccounts discovery.

### Breaking (signatures, NOT account layout)

- `mint_entry_token` drops `sequence`, adds `source_ref_hash`.
- `set_contest_lock_time` / `set_contest_conclusion_time` gain an optional `cosigner`.
- turf-monster + solana-studio must update in the same deploy that FOLLOWS the
  Squads upgrade. Rails coordination owed: entry-token PDA from
  `sha256(source_ref)`; per-mint globally-unique `source_ref` (operator-mint
  regression — a looped `operator_<unix_ts>` now collides); cosign path for
  post-lock amends; `grade!` must ensure a lock/conclusion is set (else settle
  reverts 6028); `error_interpreter` mappings for 6036/6037/6038.

## [0.18.0] - 2026-05-29

Adds the second derived timestamp: a contest **conclusion** marker, parallel to
the v0.17 lock. `conclusion_timestamp` is carved out of `_reserved` (24 → 16),
so the account size is again UNCHANGED and v0.17 Contest PDAs need no re-init
(zeroed bytes decode as `conclusion_timestamp == 0`).

### Added
- **`conclusion_timestamp: i64` on `Contest`** (Unix seconds; `0` = none). Once
  `Clock.unix_timestamp` passes it, the contest has concluded.
- **`set_contest_conclusion_time(new_conclusion_timestamp)` instruction (1-of-3).**
  Sets/clears the conclusion time; `0` clears it. Rejected once the contest has
  already concluded or is settled/cancelled.
- **`ContestConcluded` error (6035).**

### Changed
- **`set_contest_lock_time` now enforces the real conclusion gate**: rejects with
  `ContestConcluded` once `conclusion_timestamp` has passed (replacing the v0.17
  interim Settled/Cancelled-only proxy — that check is kept too). The lock time
  is final once the contest concludes.

## [0.17.0] - 2026-05-29

Contest locking becomes a DERIVED on-chain primitive instead of a status flip.
A contest carries a `lock_timestamp`; `enter_contest{,_with_token}` reject new
entries once the chain `Clock.unix_timestamp` passes it. No oracle — the Clock
sysvar (already used by `create_season` / `mint_entry_token`) is the time
source. **No state-size change** — `lock_timestamp: i64` is carved out of the
Contest `_reserved` padding (32 → 24 bytes), so existing Contest PDAs stay
bit-compatible (their zeroed bytes decode as `lock_timestamp == 0` = no lock).

### Added
- **`lock_timestamp: i64` on `Contest`** (Unix seconds; `0` = no lock scheduled,
  enterable indefinitely). Set at create time and adjustable afterward.
- **`set_contest_lock_time(new_lock_timestamp)` instruction (1-of-3).** Sets or
  clears the lock time. "Lock now" = pass the current chain time; `0` clears it.
  Rejected once the contest is concluded (interim guard: `Settled`/`Cancelled`
  via `ContestAlreadySettled` 6006; a dedicated conclusion timestamp will
  tighten this later).
- **`ContestLocked` error (6034).** Raised by both entry instructions when the
  lock timestamp has passed.

### Changed
- **`create_contest` takes a `lock_timestamp: i64` arg** (appended after
  `prize_pool`), stored on the Contest PDA.
- **`enter_contest` + `enter_contest_with_token`** now enforce the derived time
  gate in-handler (`Clock` is unavailable in account constraints), in addition
  to the existing `status == Open` + `current_entries < max_entries` checks.

### Removed
- **`lock_contest` (1-of-3) and `unlock_contest` (2-of-3) instructions.**
  Superseded by the derived time-lock. `ContestStatus::Locked` is now vestigial
  (kept for enum-discriminant stability; no instruction sets it). `settle_contest`
  / `cancel_contest` still accept `Open || Locked`; the `Locked` arm is dead but
  harmless. Error `ContestNotLocked` (6028) retained for numbering stability.

## [0.15.1] - 2026-05-24

Pre-mainnet audit closeout. Closes three findings from the 2026-05-24
prelaunch security audit (Jasper). No state-layout changes; existing
UserAccount / Contest / ContestEntry PDAs remain bit-identical.

### Removed
- **`migrate_user_account` instruction (C1).** The instruction was a one-time
  v0.13 → v0.14 backfill helper for the username field; it was never used
  in anger (devnet was reset, mainnet has no v0.13 accounts to migrate).
  Per the audit, the implementation lacked a wallet Signer requirement and
  didn't bind the PDA-seed `wallet` argument to the stored `user_account.wallet`
  field — so any 1-of-3 admin (including the Heroku-resident bot key) could
  rewrite any UserAccount with no consent. The instruction is removed
  outright. Future schema bumps will ship a properly-constrained realloc
  if needed.

### Added
- **`set_username` + `create_user_account` validation (C2).** Both paths now
  call a shared `validate_username` helper:
  - At least 3 non-null leading bytes (rejects short squats).
  - Every non-null byte must be printable ASCII `0x20..=0x7E` (rejects
    control chars, high-bit bytes, and the on-chain layer of homoglyph
    attacks).
  - Reserved-prefix list (case-insensitive): `admin`, `system`, `turf`,
    `vault`, `turfmonster`, `support`, `mod`, `official`, `staff`, `team`,
    `root`. Anything beginning with one of these is rejected.
  - New error codes: `UsernameReserved` (6020), `UsernameInvalidChars` (6021),
    `UsernameTooShort` (6022).
  - Rails-side uniqueness / homoglyph normalization / rate-limiting is
    unchanged. A `UsernameRegistry` PDA + per-account rate-limit is the
    v0.16 follow-up.

### Changed
- **PDA-seed-bind Contest in entry + settle instructions (H1).** All five
  Contest-touching instructions (`enter_contest`, `enter_contest_direct`,
  `enter_contest_with_token`, `enter_contest_direct_with_token`,
  `settle_contest`) now constrain the Contest account with
  `seeds = [b"contest", contest.contest_id.as_ref()], bump = contest.bump`.
  Anchor re-derives the PDA from the account's own stored contest_id+bump
  and rejects mismatches. Defense-in-depth on top of off-chain TX-verifier
  checks.

### Deprecated (kept for code-stability)
- `AccountAlreadyMigrated` (6011), `InvalidAccountData` (6012) error variants
  remain in `errors.rs` to preserve numbering — `AccountAlreadyMigrated` is
  still used by `force_close_vault`; `InvalidAccountData` becomes unused
  with `migrate_user_account` deleted but stays so subsequent variants keep
  their stable Rails-side error-code mapping.

## [0.15.0] - 2026-05-23

Pre-mainnet hardening. Bakes in deploy-day attack mitigations, adds an
emergency pause switch, caps per-user withdraw velocity, and ships an
adversarial-audit pass of plain-English instruction documentation.

### Added
- **Emergency pause** (M5). New `pause(reason: [u8; 64])` and `unpause()`
  instructions at 2-of-3 multisig. When paused, `deposit`, `withdraw`, and
  all four `enter_contest*` variants reject with `VaultPaused` (6018).
  Admin/maintenance ops remain available (settle, close, mint_entry_token,
  migrate, set_username, create_user_account, create_season, update_signers,
  force_close_vault, pause, unpause).
- **Daily withdraw cap**. New `daily_withdrawn: u64` + `daily_window_start: i64`
  on `UserAccount`. `withdraw` enforces a rolling 24h cap per user of
  `DAILY_WITHDRAW_CAP` (100_000_000 lamports = $100). Window auto-resets when
  24h has elapsed since the previous start. Rejected withdraws return
  `WithdrawDailyCapExceeded` (6019). Friction for legit large withdrawals,
  hard cap on damage if a managed-wallet key is leaked.
- **`paused: bool`** on `VaultState` (defaulted to false at initialize).
- **`INIT_AUTHORITY` constant** in `state.rs` — hardcodes the only wallet
  permitted to call `initialize`. Set to `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`
  (Alex's Phantom key; never on the server).
- **`EXPECTED_USDC_MINT` / `EXPECTED_USDT_MINT` constants**, feature-gated.
  Default = devnet test mints; `--features mainnet` = canonical Circle USDC
  (`EPjFW…`) + Tether USDT (`Es9vM…`). `initialize` rejects mints that don't
  match the build's constants.
- New error codes: `VaultPaused` (6018), `WithdrawDailyCapExceeded` (6019).
- **Mainnet Cargo feature** in `programs/turf_vault/Cargo.toml`. Mainnet
  binary must be built with `anchor build -- --features mainnet`.

### Security (pre-launch audit)
- **H1 — `initialize` frontrun mitigation.** Previously anyone who won the
  race between program deploy and the legit init TX could become vault owner
  with attacker-controlled signers and mints. Now `initialize` rejects unless
  admin == INIT_AUTHORITY AND mints == EXPECTED_USDC/USDT_MINT for the build.
  Reduces "frontrun the deploy" attack surface to zero; the only way to
  initialize is with Alex's Phantom signature + the canonical mints.
- **M5 — Emergency stop**. Audit-flagged absence of a pause/circuit breaker.
  Now an operator (2-of-3) can halt user-facing funds operations within
  one transaction's finality time when a bug or attack is detected, without
  needing a full Squads program upgrade.
- **New — withdraw daily cap**. Caps an attacker-with-stolen-keypair to
  $100/day per user instead of the full balance instantly. Even if Lazarus
  exfiltrates the MANAGED_WALLET_ENCRYPTION_KEY + DB, the on-chain rate limit
  bounds per-account damage to $100/day.

### Breaking
- **`VaultState` layout changed** — added `paused: bool` (1 byte). Existing
  vaults must be migrated via `force_close_vault` → `initialize`. On devnet:
  `bin/rails solana:init_vault FORCE_CLOSE=true && bin/rails solana:init_vault INIT=true SIGNERS=... THRESHOLD=2`.
- **`UserAccount` layout changed** — added `daily_withdrawn: u64` + `daily_window_start: i64`
  (16 bytes). Existing accounts must be migrated via `migrate_user_account`.
  `migrate_user_account` now handles both v0.13 (81 bytes, no username) and
  v0.14 (113 bytes) source layouts, extending to v0.15 (129 bytes) with
  the new daily fields initialized to zero. Migration is idempotent.
- **`initialize` now requires Alex's Phantom key**, not the Rails server's
  Alex Bot key. The `bin/rails solana:init_vault` task needs to be updated
  to either build a partially-signed init for Alex to cosign via Phantom,
  or be replaced with a one-shot CLI script that loads Alex's key. **For
  the mainnet first-deploy: Alex signs initialize from Phantom (or a local
  CLI with his key); this is a once-per-deployment event.**

### Documentation
- Plain-English module docstrings added to every instruction file. Each
  file now leads with a "what this does" summary before the technical
  details — readable by a non-Rust developer doing a code tour.
- `state.rs` field-by-field comments explaining purpose, lifecycle, and
  audit notes inline.
- `lib.rs` now carries the program's high-level model (account roles,
  auth tiers, instruction grouping) as a module-level doc comment.

### Migration procedure (devnet)
1. Build: `anchor build`
2. Deploy via Squads: `node scripts/squad-upgrade.js <buffer_addr>`
3. Force-close old vault: `bin/rails solana:init_vault FORCE_CLOSE=true`
4. Re-initialize (Alex Phantom signs):
   `bin/rails solana:init_vault INIT=true SIGNERS=F6f8h5yyn...,7ZDJp7FUHh...,CytJS23p1z... THRESHOLD=2`
5. For each existing UserAccount: `migrate_user_account` (idempotent — safe to run twice)
6. Re-pin IDL hash in turf-monster (OPSEC-014): `cp target/idl/turf_vault.json /Users/alex/projects/turf-monster/config/turf_vault.idl.json && shasum -a 256 /Users/alex/projects/turf-monster/config/turf_vault.idl.json` → set `EXPECTED_IDL_HASH`

### Mainnet first-deploy procedure
1. Build with mainnet mints: `anchor build -- --features mainnet`
2. Deploy program via Squads to a fresh mainnet program ID.
3. Alex builds + signs `initialize` from Phantom or a local CLI loaded with
   Alex's keypair. (Alex Bot CANNOT initialize — INIT_AUTHORITY rejects.)
4. Verify with `solana program show <program_id> --url mainnet-beta`.

## [0.14.0] - 2026-05-22

On-chain usernames. `UserAccount` now stores the master copy of a user's
username; the Rails app mirrors it. A new `set_username` instruction lets a
user rename their own account. The `UserAccount` layout grew — existing
accounts migrate via `migrate_user_account`.

### Added
- **`username: [u8; 32]` on `UserAccount`** — UTF-8, zero-padded; the on-chain master record for a user's display name.
- **`set_username` instruction** — sets/updates the username on a `UserAccount`. Signed by the account's own `wallet` (single-signer, PDA-gated) — no admin, no multisig. Bytes are stored verbatim; format + uniqueness are enforced by the Rails app.

### Breaking
- **`UserAccount` layout changed** — `username: [u8; 32]` added (81 → 113 bytes). Existing accounts must be migrated with `migrate_user_account`, which now reallocs the pre-v0.14.0 (81-byte) layout, preserves `seeds`, and initializes `username` empty (the Rails app backfills via `set_username`).
- **`create_user_account` takes a new `username: [u8; 32]` argument** (second positional, after `wallet`) — the username is set at account-creation time.

## [0.13.0] - 2026-05-19

OPSEC-023 — binds every contest to a single season so the seed-award
schedule can't be cherry-picked. Breaking Contest layout change; no
migration (devnet/pre-prod clean break).

### Security
- **OPSEC-023 — `Season` was unconstrained across all four enter_contest variants.** `season: Account<'info, Season>` had no `seeds` constraint and `Contest` didn't record its season, so any caller could pass an arbitrary `Season` and claim the richest `seed_schedule` — inflating `UserAccount.seeds`, levels, and future tier rewards. `Contest` now stores `season_id: u32`, set at `create_contest`; `enter_contest`, `enter_contest_direct`, `enter_contest_with_token`, and `enter_contest_direct_with_token` pin `season` with `seeds = [b"season", contest.season_id.to_le_bytes()]`.

### Breaking
- **Contest account layout changed** — `season_id: u32` added. Contest accounts created by ≤0.12.0 are not migrated; recreate test contests after deploy.
- `create_contest` takes a new `season_id` argument (second positional, after `contest_id`).

### Internal
- Audit reference: `mcritchie-studio/docs/agents/system/opsec-audit-pre-prod-2026-05-19.md` OPSEC-023.

## [0.12.0] - 2026-05-19

First release deployed through the Squads multisig (see OPSEC-002 below).
Five instruction-hardening fixes from the pre-prod audit — no account
layout changes, so no migration needed.

### Fixed
- **OPSEC-004 — enter_contest_with_token consumed tokens without owner consent (CRITICAL).** The `wallet` account was an `UncheckedAccount`; only `payer` (any 1-of-3 vault signer) had to sign. A compromised Alex Bot key could burn ANY user's `EntryTokenAccount`. `wallet` is now a required `Signer` — for managed (web2) wallets the server co-signs with the user's custodial keypair, so a leaked admin key alone is no longer sufficient.
- **OPSEC-024 — enter_contest_direct had no payer gating (HIGH).** The `vault_state` account lacked the `is_signer(&payer.key())` constraint that every other entry instruction has, diverging from the documented "1-of-3" auth model and letting anyone construct a direct-entry TX. Constraint added.
- **OPSEC-025 — create_contest payout sum used wrapping arithmetic (HIGH).** `payout_amounts.iter().sum::<u64>()` wraps silently; `[u64::MAX, 1]` sums to 0 and would pass an equality check against `prizes=0`. Now a `checked_add` fold → `Overflow`.
- **OPSEC-026 — force_close_vault was replayable forever (HIGH).** The migration-only instruction had no guard against running on a current-schema vault — 2 compromised signers could brick the live vault at any time. Now refuses when `data.len() == 8 + VaultState::INIT_SPACE` (`AccountAlreadyMigrated`).
- **OPSEC-027 — update_signers could lock out the multisig (HIGH).** Two compromised signers could rotate to 3 attacker addresses, stranding the legitimate third party. Now requires continuity — at least one of the two cosigners authorizing the update must remain in the new set (`SignerContinuityRequired`, 6017).

### Added
- New error `SignerContinuityRequired` (6017).
- Tests: `rejects create_contest with overflowing payout_amounts (OPSEC-025)`, `rejects update_signers that drops all current cosigners (OPSEC-027)`. The three `enter_contest_with_token` tests now sign as `wallet`. 43 tests pass.

### Operations
- **OPSEC-002 — program upgrade authority migrated to a Squads 2-of-3 multisig (devnet).**
  - Upgrade authority moved from the single keypair `4AQMNwhyZtsaCLx3Dv9G5a2rXaJ6M221FYQw6sommRWz` to the Squads V4 vault PDA `BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC`. A program upgrade now requires the same 2-of-3 cosign (Alex Bot / Alex / Mason) as a treasury op — closes the single-key code-deployment risk.
  - Squad: multisig PDA `7nRuVw3VZFC6z85tYVDitPnaUHZCkqLpJRSTBNtPmtZB`, threshold 2, autonomous (config changes require a member vote). Squads V4 program `SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf`.
  - Migration was verified end-to-end: created the Squad, proved vault execution with a trivial transaction, transferred authority, then rehearsed a full no-op program upgrade through the Squad (propose → approve ×2 → execute). turf_vault re-deployed at slot 463533624 via the Squad path.
  - **`anchor deploy` no longer works.** Upgrades go through `scripts/squad-upgrade.js` — see CLAUDE.md "Deploying an upgrade".
  - Known caveat: while operated autonomously (Alex Bot + Mason keys both in the operator's 1Password), the 2-of-3 is effectively single-trust-domain. Genuine 2-of-3 protection requires Alex + Mason to hold keys in separate domains — tracked as a follow-up.

## [0.11.1] - 2026-05-19

### Fixed
- **OPSEC-003 — settle_contest duplicate-entry double payout (CRITICAL).**
  - Settlement loop now rejects any `Vec<Settlement>` containing the same `(wallet, entry_num)` pair twice. Previously, two iterations with the same pair would deserialize, mutate, and serialize the same `UserAccount` PDA in sequence — the second pass read the first pass's write and added the payout again, bypassing the `total_payouts <= entry_fees + prizes` cap on the actual user balance.
  - Additionally, `settle_contest` now refuses to mutate any `ContestEntry` whose `status != EntryStatus::Active`. Defense-in-depth against any future second-settle path; today this also blocks the only way the duplicate-entry bug could have shown up across two separate settle calls.
  - Errors: duplicates raise `DuplicateEntry` (6008). Already-settled entries raise `ContestAlreadySettled` (6007).
  - New test: `rejects settlement with duplicate (wallet, entry_num) pair (v0.11.1)`.

### Internal
- Audit reference: `mcritchie-studio/docs/agents/system/opsec-audit-pre-prod-2026-05-19.md` OPSEC-003.

## [Unreleased] - 2026-05-18 (post-v0.11.0)

### Changed
- **Program ID migrated**: `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J` → `Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT` on devnet.
  - `target/deploy/turf_vault-keypair.json` had drifted from `declare_id!()` (regenerated on May 16). Initial `anchor deploy` after the build minted a fresh program at the new keypair address; every instruction call returned 4100 (DeclaredProgramIdMismatch).
  - Adopted the new ID end-to-end (lib.rs `declare_id!()` + Anchor.toml `[programs.{localnet,devnet}]`) since the original `7Hy8…r2J` upgrade authority `9Fy8P3DvKBh3awt1wr27g4CDh47oDqmJR2FAAQ1bc69D` is no longer in our possession.
  - ~3.45 SOL of rent stays locked at the orphaned `7Hy8…r2J` forever (devnet only).
  - Fresh on-chain state on the new program: VaultState `FYBTB5pwoSxN4CF5M45gW3e8hwMNFit6phbgyd4vpWAn` (2-of-3 Alex Bot / Alex / Mason), Season 1 PDA `C88QKhevowD7c3xQDZ3grfdHbpk4FeyYDtdkjz4nz924` (schedule `[25, 19, 14, 10, 7]`), IDL account `66fFnyBykZRKrbU3dGzkd8udoadgMtH2u9XCj9nA5x75`.

## [0.11.0] - 2026-05-18

### Added
- `Season` PDA — represents a contest season with an immutable per-entry seed-award schedule
  - PDA seeds: `[b"season", season_id.to_le_bytes()]` (u32 LE, 4 bytes)
  - Fields: `season_id` (u32), `name` ([u8; 32], UTF-8 zero-padded), `seed_schedule` ([u64; 5]), `start_at` (i64), `created_at` (i64), `bump` (u8)
  - `Season::LEN = 101` bytes (8 discriminator + 93 data)
- `create_season(season_id: u32, name: [u8; 32], seed_schedule: [u64; 5], start_at: i64)` instruction
  - Admin (any 1-of-3 vault signer) creates a season; PDA-collision on duplicate `season_id` is rejected via Anchor's `init` constraint
- 6 new tests covering: season creation + field assertions, duplicate `season_id` rejection, non-admin rejection, entry index 0 awards 25, cumulative entries 0/1/2 award 25+19+14=58, entry index 7 clamps to slot 4 (awards 7)

### Changed
- All 4 entry instructions (`enter_contest`, `enter_contest_direct`, `enter_contest_with_token`, `enter_contest_direct_with_token`) now require a `season` account and award seeds from `season.seed_schedule[entry_num.min(4) as usize]` instead of the hardcoded `+65`
  - Entries 0-4 use slots 0-4 respectively; entries 5+ clamp to slot 4
  - Seed addition still uses `checked_add` for overflow safety
  - `season` is appended at the end of each Context struct, before `system_program` (account list order matters for Rails TX builders)
- Existing entry-test assertions updated from `+65` to the appropriate schedule slot (entry_num=1 → `seed_schedule[1]` = 19)

### Migration / breaking notes
- **Breaking instruction signature**: all 4 entry instructions now require an additional `season` account at the end of their account list (before `system_program`). Rails TX builders must be updated.
- The new account ordering is documented per-instruction in the source file headers.

## [0.10.0] - 2026-05-18

### Added
- `enter_contest_with_token(entry_num: u32)` — managed-wallet entry funded by an `EntryTokenAccount`
  - Atomically consumes the supplied entry token (sets `consumed = true`, `consumed_at = now`)
  - Skips USDC entry fee entirely (token IS the payment); `contest.entry_fees` is NOT incremented
  - Still awards 65 seeds — token entries progress the user's level identically to paid entries
  - Verifies `entry_token.owner == wallet.key()` and `entry_token.consumed == false` before consuming
  - Auth: any 1-of-3 vault signer (same as `enter_contest`)
- `enter_contest_direct_with_token(entry_num: u32)` — Phantom-direct entry funded by an `EntryTokenAccount`
  - User signs to authorize token consumption (no USDC transfer occurs)
  - Same consume + seeds-award semantics as `enter_contest_with_token`
  - Auth: user signs; admin pays PDA rent
- New errors: `EntryTokenAlreadyConsumed` (6015), `EntryTokenWrongOwner` (6016)
- 5 new tests covering: managed-path happy path, double-consume rejection, wrong-owner rejection, backwards-compat for `enter_contest`, direct-path happy path

### Design notes
- Implemented as two new variant instructions (rather than optional accounts on the existing pair) for symmetry with the existing `enter_contest` / `enter_contest_direct` split. Rails can build the right TX based on whether a token is being redeemed.
- The pair `(enter_contest, enter_contest_with_token)` covers managed wallets; `(enter_contest_direct, enter_contest_direct_with_token)` covers Phantom wallets. No optional-account complexity on either path.

## [0.9.0] - 2026-05-18

### Added
- `EntryTokenAccount` PDA — represents a pre-purchased contest entry token issued by the admin
  - PDA seeds: `[b"entry_token", owner.as_ref(), sequence.to_le_bytes()]` (8-byte little-endian u64)
  - Fields: `owner`, `source` (u8: 0=operator, 1=stripe, 2=moonpay), `source_ref` ([u8; 64]), `consumed`, `consumed_at` (Option<i64>), `created_at`, `bump`
  - `EntryTokenAccount::LEN = 124` bytes (8 discriminator + 116 data)
- `mint_entry_token(sequence: u64, source: u8, source_ref: [u8; 64])` instruction
  - Admin (any vault signer, 1-of-3) mints a token for any user wallet (recipient is not a signer)
  - Admin pays SOL rent for the PDA
  - Constants exposed in `state::entry_token_source::{OPERATOR, STRIPE, MOONPAY}`
- 4 new tests covering mint success, sequence-distinct PDAs, non-signer rejection, and PDA-collision rejection
- Consume instruction will land in a follow-up version (out of scope here)

## [0.1.0] - 2026-04-02

### Added
- Initial Anchor program with 8 instructions: initialize, create_user_account, deposit, withdraw, create_contest, enter_contest, settle_contest, close_contest
- VaultState, UserAccount, Contest, ContestEntry account structures
- USDC + USDT dual-mint deposit/withdraw support (6 decimals)
- Contest lifecycle: create → enter → settle → close
- Payout basis points system (up to 10 ranks, sum ≤ 10000 bps)
- Admin-funded bonus pool on top of entry fees
- Settlement via remaining_accounts with PDA verification
- Checked arithmetic on all balance operations
- Full TypeScript test suite (19 tests covering all instructions + error cases)
- Deployed to devnet: `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J`
