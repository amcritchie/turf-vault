# Changelog

All notable changes to TurfVault are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased] - 2026-05-19 (post-v0.11.1)

### Security / Operations
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
