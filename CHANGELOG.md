# Changelog

All notable changes to TurfVault are documented here. Format based on [Keep a Changelog](https://keepachangelog.com/).

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
