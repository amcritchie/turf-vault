# TurfVault

Solana escrow program for contest entry fees and prize distribution. Built with [Anchor](https://www.anchor-lang.com/).

**Program ID**: `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J`

![Anchor 0.32.1](https://img.shields.io/badge/Anchor-0.32.1-blue)
![Solana](https://img.shields.io/badge/Solana-Devnet-purple)
![License: MIT](https://img.shields.io/badge/License-MIT-green)

## Overview

TurfVault is the on-chain backend for [Turf Monster](https://turf.mcritchie.studio), a sports pick'em app. It implements a "DeFi mullet" — a traditional Rails web app on top, Solana smart contract underneath.

> **Part of the McRitchie ecosystem** — see [`ECOSYSTEM.md`](https://github.com/amcritchie/mcritchie-studio/blob/main/docs/ECOSYSTEM.md) for the 5-repo map; [`house-burn-down.md`](https://github.com/amcritchie/mcritchie-studio/blob/main/docs/agents/system/house-burn-down.md) for fresh-Mac recovery.

Users deposit USDC/USDT into the vault, enter contests by paying entry fees from their balance, and receive payouts when contests settle. All token custody and prize math happen on-chain; the Rails app handles UX and game logic.

## Architecture

```
VaultState (PDA: "vault")
├── admin (Pubkey)
├── usdc_mint / usdt_mint
├── vault_usdc / vault_usdt (token accounts)
│
├── UserAccount (PDA: "user" + wallet)
│   ├── balance, total_deposited, total_withdrawn, total_won
│   └── wallet (Pubkey)
│
└── Contest (PDA: "contest" + contest_id)
    ├── entry_fee, max_entries, prize_pool, bonus
    ├── payout_bps (Vec<u16>, max 10 ranks)
    ├── status: Open → Locked → Settled
    │
    └── ContestEntry (PDA: "entry" + contest_id + wallet + entry_num)
        ├── status: Active → Won/Lost
        ├── rank, payout
        └── wallet, entry_num
```

### PDA Seeds

| Account | Seeds |
|---------|-------|
| VaultState | `["vault"]` |
| UserAccount | `["user", wallet]` |
| Contest | `["contest", contest_id]` |
| ContestEntry | `["entry", contest_id, wallet, entry_num (LE bytes)]` |

## Instructions

| Instruction | Params | Auth | Description |
|-------------|--------|------|-------------|
| `initialize` | — | Admin (signer) | Create vault, set mints, init token accounts |
| `create_user_account` | `wallet` | Any signer (payer) | Create per-user balance account |
| `deposit` | `amount` | User (signer) | Transfer tokens to vault, credit balance |
| `withdraw` | `amount` | User (signer) | Debit balance, transfer tokens from vault |
| `create_contest` | `contest_id, entry_fee, max_entries, payout_bps, bonus` | Admin | Create contest with payout tiers |
| `enter_contest` | `entry_num` | Payer (signer) | Debit entry fee, add to prize pool |
| `settle_contest` | `settlements: Vec<Settlement>` | Admin | Assign ranks/payouts, credit winners |
| `close_contest` | — | Admin | Close settled contest, reclaim rent |

### Settlement Struct

```rust
pub struct Settlement {
    pub wallet: Pubkey,
    pub entry_num: u32,
    pub rank: u32,
    pub payout: u64,
}
```

Settlement accounts are passed as `remaining_accounts` — pairs of `[user_account, contest_entry]` per settlement, verified via PDA derivation.

## Account State

### VaultState
| Field | Type | Description |
|-------|------|-------------|
| `admin` | Pubkey | Vault administrator |
| `usdc_mint` | Pubkey | Accepted USDC mint |
| `usdt_mint` | Pubkey | Accepted USDT mint |
| `vault_usdc` | Pubkey | Vault USDC token account |
| `vault_usdt` | Pubkey | Vault USDT token account |
| `bump` | u8 | PDA bump seed |

### UserAccount
| Field | Type | Description |
|-------|------|-------------|
| `wallet` | Pubkey | Owner wallet |
| `balance` | u64 | Current balance (6 decimals) |
| `total_deposited` | u64 | Lifetime deposits |
| `total_withdrawn` | u64 | Lifetime withdrawals |
| `total_won` | u64 | Total winnings received |
| `bump` | u8 | PDA bump seed |

### Contest
| Field | Type | Description |
|-------|------|-------------|
| `contest_id` | [u8; 32] | SHA256 of Rails slug |
| `entry_fee` | u64 | Fee per entry (6 decimals) |
| `max_entries` | u32 | Maximum entries allowed |
| `current_entries` | u32 | Current entry count |
| `prize_pool` | u64 | Sum of entry fees |
| `bonus` | u64 | Admin-funded bonus |
| `status` | ContestStatus | Open / Locked / Settled |
| `payout_bps` | Vec\<u16\> | Basis points per rank (max 10, sum ≤ 10000) |
| `admin` | Pubkey | Contest creator |
| `bump` | u8 | PDA bump seed |

### ContestEntry
| Field | Type | Description |
|-------|------|-------------|
| `contest_id` | [u8; 32] | Parent contest |
| `wallet` | Pubkey | Entry owner |
| `entry_num` | u32 | Entry identifier (supports multiple per user) |
| `status` | EntryStatus | Active / Won / Lost |
| `rank` | u32 | Final placement |
| `payout` | u64 | Winnings (6 decimals) |
| `bump` | u8 | PDA bump seed |

## Contest Flow

```
Create → Enter → Settle → Close
  │        │        │        │
  │        │        │        └─ Reclaim rent (admin)
  │        │        └─ Assign ranks, credit winners (admin)
  │        └─ Debit entry fee, build prize pool (user)
  └─ Set fee, max entries, payout tiers (admin)
```

1. **Create**: Admin creates contest with entry fee, max entries, payout basis points, and optional bonus
2. **Enter**: Users pay entry fee from their vault balance. Prize pool accumulates on-chain
3. **Settle**: Admin submits settlement array with rank + payout per entry. Winners credited, losers marked. Total payouts validated against pool + bonus
4. **Close**: Admin closes the settled contest account, reclaiming rent to admin wallet

## Token Support

- **USDC** and **USDT** — both 6 decimals (standard Solana SPL tokens)
- Vault holds dual token accounts, one per mint
- Deposit/withdraw validates mint against vault's accepted mints
- All amounts stored as `u64` with 6 decimal precision (1 USDC = 1,000,000)

## Development

See [CLAUDE.md](./CLAUDE.md) for detailed development context including the dual admin system, PDA patterns, error codes, integration with Turf Monster, and AI agent instructions.

### Prerequisites

- [Rust](https://rustup.rs/) 1.89+
- [Solana CLI](https://docs.solanalabs.com/cli/install) 2.x
- [Anchor CLI](https://www.anchor-lang.com/docs/installation) 0.32.1
- [Node.js](https://nodejs.org/) + Yarn

### Build

```bash
anchor build
```

### Test

```bash
anchor test
```

Tests run against a local validator and cover all 8 instructions with 19 test cases including error scenarios.

### Deploy

```bash
# Devnet
solana config set --url devnet
anchor deploy --provider.cluster devnet

# Verify
solana program show 7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J
```

## Versioning

This project uses semantic versioning with git tags and a [CHANGELOG](./CHANGELOG.md).

- **MAJOR**: Breaking account layout changes (requires migration)
- **MINOR**: New instructions or features
- **PATCH**: Bug fixes, validation improvements

Each deploy is tagged (e.g. `v0.1.0`) and documented in the changelog. See `Cargo.toml` for the current version.

## Security

- **Admin auth**: All contest management (create, settle, close) requires vault admin signature
- **PDA verification**: Settlement uses manual PDA derivation to verify all remaining accounts
- **Checked arithmetic**: All math uses `checked_add`/`checked_sub` with overflow errors
- **Payout cap**: Settlement validates total payouts ≤ prize_pool + bonus
- **Payout BPS cap**: Sum of payout basis points must be ≤ 10,000
- **Mint validation**: Deposits/withdrawals only accept configured USDC/USDT mints

## Related

- [Turf Monster](https://turf.mcritchie.studio) — Rails pick'em app that integrates with this vault
- [Anchor Framework](https://www.anchor-lang.com/) — Solana development framework

## License

MIT
