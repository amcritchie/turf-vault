# TurfVault

Solana escrow program for contest entry fees and prize distribution. Built with [Anchor](https://www.anchor-lang.com/).

**Program ID**: `Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT`

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
├── signers ([Pubkey; 3]) / threshold (u8)
├── usdc_mint / usdt_mint
├── vault_usdc / vault_usdt (token accounts)
│
├── UserAccount (PDA: "user" + wallet)
│   ├── balance, total_deposited, total_withdrawn, total_won, seeds
│   └── wallet (Pubkey)
│
├── Season (PDA: "season" + season_id)
│   └── name, seed_schedule ([u64; 5]), start_at
│
├── EntryTokenAccount (PDA: "entry_token" + owner + sequence)
│   └── source, source_ref, consumed, consumed_at
│
└── Contest (PDA: "contest" + contest_id)
    ├── entry_fee, max_entries, entry_fees, prizes, season_id
    ├── payout_amounts (Vec<u64>, max 10 ranks)
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
| Season | `["season", season_id (u32 LE bytes)]` |
| EntryTokenAccount | `["entry_token", owner, sequence (u64 LE bytes)]` |

## Instructions

| Instruction | Params | Auth | Description |
|-------------|--------|------|-------------|
| `initialize` | `signers: [Pubkey; 3], threshold: u8` | Signer (payer) | Create vault, set mints + signers, init token accounts |
| `create_user_account` | `wallet` | Any signer (payer) | Create per-user balance account |
| `deposit` | `amount` | User (signer) | Transfer tokens to vault, credit balance |
| `withdraw` | `amount` | User (signer) | Debit balance, transfer tokens from vault |
| `create_season` | `season_id, name, seed_schedule, start_at` | 1-of-3 | Create a season with an immutable seed-award schedule |
| `create_contest` | `contest_id, season_id, entry_fee, max_entries, payout_amounts, prizes` | 1-of-3 | Create contest with payout tiers, bound to a season |
| `enter_contest` | `entry_num` | 1-of-3 | Debit entry fee from balance (managed wallets) |
| `enter_contest_direct` | `entry_num` | 1-of-3 | User signs USDC transfer from their ATA (Phantom wallets) |
| `mint_entry_token` | `sequence, source, source_ref` | 1-of-3 | Mint a pre-purchased entry token for a wallet |
| `enter_contest_with_token` | `entry_num` | 1-of-3 + `wallet` | Managed-wallet entry funded by an entry token |
| `enter_contest_direct_with_token` | `entry_num` | User (signer) | Phantom-direct entry funded by an entry token |
| `settle_contest` | `settlements: Vec<Settlement>` | 2-of-3 | Assign ranks/payouts, credit winners |
| `close_contest` | — | 1-of-3 | Close settled contest, reclaim rent |
| `migrate_user_account` | — | 1-of-3 | Resize a legacy UserAccount PDA to the current layout |
| `update_signers` | `new_signers: [Pubkey; 3], new_threshold: u8` | 2-of-3 | Rotate multisig signers / change threshold |
| `force_close_vault` | — | 2-of-3 | Migration-only: close the vault, bypassing deserialization |

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
| `signers` | [Pubkey; 3] | The three multisig signers |
| `threshold` | u8 | Required sigs for treasury ops (2) |
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
| `seeds` | u64 | Seeds awarded per entry (from the contest's Season schedule) |
| `bump` | u8 | PDA bump seed |

### Contest
| Field | Type | Description |
|-------|------|-------------|
| `contest_id` | [u8; 32] | SHA256 of Rails slug |
| `prizes` | u64 | Guaranteed prize amount the admin pre-funds (6 decimals) |
| `entry_fee` | u64 | Fee per entry (6 decimals) |
| `entry_fees` | u64 | Accumulated entry fees collected |
| `max_entries` | u32 | Maximum entries allowed |
| `current_entries` | u32 | Current entry count |
| `status` | ContestStatus | Open / Locked / Settled |
| `payout_amounts` | Vec\<u64\> | USDC amount per rank (max 10, must sum to `prizes`) |
| `admin` | Pubkey | Payer pubkey (created the contest) |
| `creator` | Pubkey | Wallet that funded the `prizes` USDC |
| `season_id` | u32 | Season this contest is bound to (OPSEC-023) |
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
  │        └─ Debit entry fee, accumulate entry fees (user)
  └─ Set fee, max entries, payout tiers, pre-fund prizes (admin)
```

1. **Create**: Admin creates a contest with entry fee, max entries, payout amounts, the bound season, and a pre-funded `prizes` pool
2. **Enter**: Users pay the entry fee from their vault balance (or redeem an entry token). Entry fees accumulate on-chain
3. **Settle**: Admin submits a settlement array with rank + payout per entry. Winners credited, losers marked. Total payouts validated against `entry_fees + prizes`
4. **Close**: Admin closes the settled contest account, reclaiming rent to the admin wallet

## Token Support

- **USDC** and **USDT** — both 6 decimals (standard Solana SPL tokens)
- Vault holds dual token accounts, one per mint
- Deposit/withdraw validates mint against vault's accepted mints
- All amounts stored as `u64` with 6 decimal precision (1 USDC = 1,000,000)

## Development

See [CLAUDE.md](./CLAUDE.md) for detailed development context including the 2-of-3 multisig system, PDA patterns, error codes, integration with Turf Monster, and AI agent instructions.

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

Tests run against a local validator and cover all 16 instructions with 44 test cases including error scenarios.

### Deploy

The program upgrade authority is a Squads V4 2-of-3 multisig (OPSEC-002, 2026-05-19), so **`anchor deploy` no longer works**. Upgrades go through the Squad via `scripts/squad-upgrade.js` — see the "Deploying an upgrade" section in [CLAUDE.md](./CLAUDE.md) for the full procedure.

```bash
# Verify the deployed program
solana program show Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT --url devnet
```

## Versioning

This project uses semantic versioning with git tags and a [CHANGELOG](./CHANGELOG.md).

- **MAJOR**: Breaking account layout changes (requires migration)
- **MINOR**: New instructions or features
- **PATCH**: Bug fixes, validation improvements

Each deploy is tagged (e.g. `v0.1.0`) and documented in the changelog. See `Cargo.toml` for the current version.

## Security

- **2-of-3 multisig**: Treasury ops (settle, force_close, update_signers) require two distinct signers; routine ops require any 1-of-3
- **Squads upgrade authority**: Program upgrades require a Squads V4 2-of-3 cosign (OPSEC-002) — no single-key code deployment
- **PDA verification**: Settlement uses manual PDA derivation to verify all remaining accounts
- **Checked arithmetic**: All math uses `checked_add`/`checked_sub` with overflow errors
- **Payout cap**: Settlement validates total payouts ≤ `entry_fees + prizes`
- **Payout tier check**: `payout_amounts` must sum exactly to the contest's `prizes`
- **Mint validation**: Deposits/withdrawals only accept configured USDC/USDT mints

## Related

- [Turf Monster](https://turf.mcritchie.studio) — Rails pick'em app that integrates with this vault
- [Anchor Framework](https://www.anchor-lang.com/) — Solana development framework

## License

MIT
