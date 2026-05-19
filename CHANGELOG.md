# Changelog

All notable changes to TurfVault are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/).

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
