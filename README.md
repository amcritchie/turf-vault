# TurfVault

Solana escrow program for contest entry fees and prize distribution. Built with [Anchor](https://www.anchor-lang.com/).

**Current deployment**: see [`docs/CURRENT_DEPLOYMENT.md`](docs/CURRENT_DEPLOYMENT.md) for live devnet/mainnet program IDs, signer set, and upgrade authority.

**Docs index**: see [`docs/README.md`](docs/README.md) before following historical specs, audits, or generated reports.

![Anchor 0.32.1](https://img.shields.io/badge/Anchor-0.32.1-blue)
![Solana](https://img.shields.io/badge/Solana-Devnet-purple)
![License: MIT](https://img.shields.io/badge/License-MIT-green)

## Overview

TurfVault is the on-chain backend for [Turf Monster](https://app.turfmonster.media), a sports pick'em app. It implements a "DeFi mullet" — a traditional Rails web app on top, Solana smart contract underneath.

> **Part of the McRitchie ecosystem** — see [`ECOSYSTEM.md`](https://github.com/amcritchie/mcritchie-studio/blob/main/docs/ECOSYSTEM.md) for the 5-repo map; [`house-burn-down.md`](https://github.com/amcritchie/mcritchie-studio/blob/main/docs/agents/system/house-burn-down.md) for fresh-Mac recovery.

TurfVault uses a server-facilitated self-custody model. User funds live in each user's own SPL token account (ATA), not in a pooled vault balance. Paid entries transfer the entry fee from the user ATA into a per-currency operator-revenue ATA; contest prizes are pre-funded into a per-contest prize-pool ATA and paid directly to winners on settlement. Rails handles UX and game logic, but the money-moving state transitions happen on-chain.

## Architecture

```
VaultState (PDA: "vault")
├── signers ([Pubkey; 3]) / threshold (u8)
├── payout_mint (USDC)
├── treasury_authority (Squads vault PDA)
├── accepted_currencies[16] (mint, op_rev_ata, kind, active)
├── paused
│
├── UserAccount (PDA: "user" + wallet)
│   ├── username ([u8; 32]), seeds
│   ├── entries, wins, cashes, total_won
│   └── wallet
│
├── Season (PDA: "season" + season_id)
│   └── name, seed_schedule ([u64; 5]), quest_seeds ([u64; 16]), start_at
│
├── EntryTokenAccount (PDA: "entry_token" + sha256(source_ref))
│   └── source, source_ref, consumed, consumed_at
│
└── Contest (PDA: "contest" + contest_id)
    ├── prize_pool, entry_fee_by_currency[16], entry_fees[16]
    ├── max_entries, current_entries, season_id
    ├── payout_amounts (Vec<u64>, max 10 ranks)
    ├── status: Open → Locked → Settled/Cancelled
    ├── lock_timestamp, conclusion_timestamp
    │
    └── ContestEntry (PDA: "entry" + contest_id + wallet + entry_num)
        ├── status: Active → Won/Lost, currency_idx
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
| EntryTokenAccount | `["entry_token", sha256(source_ref)]` |

## Instructions

| Instruction | Params | Auth | Description |
|-------------|--------|------|-------------|
| `initialize` | `signers, threshold, treasury_authority` | `INIT_AUTHORITY` on mainnet | Create vault, pin payout mint + treasury authority, register USDC/USDT slots |
| `update_signers` | `new_signers` | 2-of-3 | Rotate signer pubkeys; threshold remains pinned at 2 |
| `register_currency` | `kind` | 2-of-3 | Add a mint to the currency registry and initialize its operator-revenue ATA |
| `deactivate_currency` | `currency_idx` | 2-of-3 | Disable a currency slot without reclaiming it |
| `pause` | `reason: [u8; 64]` | 2-of-3 | Block `enter_contest` and `enter_contest_with_token` |
| `unpause` | — | 2-of-3 | Clear the pause flag |
| `create_user_account` | `wallet, username` | Permissionless payer | Create per-wallet stat/username account |
| `set_username` | `username` | User signer | Update the wallet owner's username |
| `admin_create_user_account` | `wallet, username` | Payer + 1-of-3 | Create a user account with reserved-prefix waiver |
| `admin_set_username` | `username` | User signer + 1-of-3 | Set a reserved-prefix username with admin authorization |
| `create_season` | `season_id, name, seed_schedule, quest_seeds, start_at` | 1-of-3 | Create immutable seed schedule for a season |
| `create_contest` | `contest_id, season_id, entry_fee_by_currency, max_entries, payout_amounts, prize_pool, lock_timestamp` | 1-of-3 payer + creator | Initialize contest and fund its prize-pool ATA |
| `set_contest_lock_time` | `new_lock_timestamp` | 1-of-3 | Set or clear the derived entry lock time |
| `set_contest_conclusion_time` | `new_conclusion_timestamp` | 1-of-3 | Set or clear the contest conclusion timestamp |
| `enter_contest` | `entry_num, currency_idx` | User signer + 1-of-3 payer | Paid entry: transfer user ATA funds to operator-revenue ATA |
| `enter_contest_with_token` | `entry_num` | User signer + 1-of-3 payer | Entry funded by consuming an `EntryTokenAccount` |
| `mint_entry_token` | `source, source_ref, source_ref_hash` | 1-of-3 | Mint an idempotent pre-purchased entry voucher |
| `grant_seeds` | `amount, kind, invitee` | 1-of-3 | Grant quest/referral seeds outside the normal entry flow |
| `settle_contest` | `settlements: Vec<Settlement>` | 2-of-3 | Pay winners from the contest prize-pool ATA and update stats |
| `cancel_contest` | — | 2-of-3 | Refund the live prize-pool balance to the creator |
| `close_contest` | — | 1-of-3 | Close settled/cancelled contest accounts and reclaim rent |
| `sweep_operator_revenue` | `amount` | 2-of-3 | Move operator-revenue funds to the pinned treasury ATA |

### Settlement Struct

```rust
pub struct Settlement {
    pub wallet: Pubkey,
    pub entry_num: u32,
    pub rank: u32,
    pub payout: u64,
}
```

Settlement accounts are passed as `remaining_accounts` — triples of `[user_account, contest_entry, winner_usdc_ata]` per settlement, verified via PDA/ATA derivation.

## Account State

### VaultState
| Field | Type | Description |
|-------|------|-------------|
| `signers` | [Pubkey; 3] | The three multisig signers |
| `threshold` | u8 | Required sigs for treasury ops (2) |
| `payout_mint` | Pubkey | Pinned USDC payout mint |
| `treasury_authority` | Pubkey | Squads vault PDA that owns treasury sweep destination |
| `accepted_currencies` | [AcceptedCurrency; 16] | Registry of accepted entry currencies and operator-revenue ATAs |
| `bump` | u8 | PDA bump seed |
| `paused` | bool | Circuit breaker — when true, user-facing ops are blocked. Set via `pause` / cleared via `unpause` (both 2-of-3) |

### UserAccount
| Field | Type | Description |
|-------|------|-------------|
| `wallet` | Pubkey | Owner wallet |
| `username` | [u8; 32] | UTF-8 username, right-padded with `0x00`. Set on create or via `set_username` (user-signed) |
| `seeds` | u64 | Loyalty seeds earned through entries and explicit grants |
| `entries` | u32 | Lifetime successful entries |
| `wins` | u32 | Lifetime first-place finishes |
| `cashes` | u32 | Lifetime payout finishes |
| `total_won` | u64 | Lifetime USDC payouts received |
| `bump` | u8 | PDA bump seed |

### Contest
| Field | Type | Description |
|-------|------|-------------|
| `contest_id` | [u8; 32] | SHA256 of Rails slug |
| `prize_pool` | u64 | USDC prize pool pre-funded by the creator |
| `entry_fee_by_currency` | [u64; 16] | Per-currency entry fee schedule |
| `entry_fees` | [u64; 16] | Per-currency operator-revenue tally |
| `max_entries` | u32 | Maximum entries allowed |
| `current_entries` | u32 | Current entry count |
| `status` | ContestStatus | Open / Locked / Settled |
| `payout_amounts` | Vec\<u64\> | USDC amount per rank (max 10, must sum to `prize_pool`) |
| `admin` | Pubkey | Payer pubkey (created the contest) |
| `creator` | Pubkey | Wallet that funded the `prize_pool` USDC |
| `season_id` | u32 | Season this contest is bound to (OPSEC-023) |
| `lock_timestamp` | i64 | Derived entry lock time; `0` means no scheduled lock |
| `conclusion_timestamp` | i64 | Derived conclusion time after which lock time cannot change |
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
  │        └─ Transfer entry fee to operator revenue (user)
  └─ Set fee, max entries, payout tiers, pre-fund prize pool (admin/creator)
```

1. **Create**: Admin creates a contest with per-currency entry fees, max entries, payout amounts, the bound season, and a pre-funded USDC `prize_pool`
2. **Enter**: Users pay from their own ATA into operator revenue, or redeem an entry token. There is no vault balance debit
3. **Settle**: Admin submits a settlement array with rank + payout per entry. Winners receive direct USDC transfers from the prize-pool ATA. Total payouts are capped by `prize_pool`
4. **Close**: Admin closes the settled contest account, reclaiming rent to the admin wallet

## Token Support

- **USDC** and **USDT** — both 6 decimals (standard Solana SPL tokens)
- Slot 0 is the payout mint (USDC); slot 1 is USDT; slots 2-15 can be registered later
- Paid entries validate the selected currency slot before transferring from the user's ATA to operator revenue
- All amounts stored as `u64` with 6 decimal precision (1 USDC = 1,000,000)

## Development

See [`docs/CURRENT_DEPLOYMENT.md`](docs/CURRENT_DEPLOYMENT.md) for live deployment identity. `CLAUDE.md` remains legacy migration context while neutral app docs are being extracted.

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

Tests run against a local validator. The TypeScript suite is being realigned from the old deposit/withdraw contract shape to the current self-custody instruction surface; check the latest test status before treating it as launch evidence.

### Deploy

The program upgrade authority is a Squads V4 2-of-3 multisig (OPSEC-002, 2026-05-19), so **`anchor deploy` is not the upgrade path for an existing deployed program**. Upgrades go through the Squad via `scripts/squad-upgrade.js`; see [`docs/CURRENT_DEPLOYMENT.md`](docs/CURRENT_DEPLOYMENT.md) for the current authority and upgrade rule.

```bash
# Verify the deployed program
solana program show EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ --url devnet
```

## Versioning

This project uses semantic versioning with git tags and a [CHANGELOG](./CHANGELOG.md).

- **MAJOR**: Breaking account layout changes (requires migration)
- **MINOR**: New instructions or features
- **PATCH**: Bug fixes, validation improvements

Each deploy is tagged (e.g. `v0.1.0`) and documented in the changelog. See `Cargo.toml` for the current version.

## Security

- **2-of-3 multisig**: Treasury/governance ops (`settle_contest`, `cancel_contest`, `sweep_operator_revenue`, currency registry changes, pause/unpause, signer rotation) require two distinct signers; routine ops require any 1-of-3
- **Squads upgrade authority**: Program upgrades require a Squads V4 2-of-3 cosign (OPSEC-002) — no single-key code deployment
- **PDA verification**: Settlement uses manual PDA derivation to verify all remaining accounts
- **Checked arithmetic**: All math uses `checked_add`/`checked_sub` with overflow errors
- **Payout cap**: Settlement validates total payouts ≤ `prize_pool`
- **Payout tier check**: `payout_amounts` must sum exactly to the contest's `prize_pool`
- **Mint validation**: Paid entries use registered active currency slots and canonical mint accounts

## Related

- [Turf Monster](https://app.turfmonster.media) — Rails pick'em app that integrates with this vault
- [Anchor Framework](https://www.anchor-lang.com/) — Solana development framework

## License

MIT
