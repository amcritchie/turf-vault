# TurfVault — Development Instructions

## Project Overview

Anchor smart contract for contest escrow on Solana. Backend for Turf Monster (Rails pick'em app).

- **Program ID (devnet)**: `EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ` — fresh deploy 2026-05-27 (the prior `Dx8u…GaCT` is orphaned with ~4 SOL ProgramData rent locked under Squads authority).
- **Framework**: Anchor 0.32.1
- **Rust**: 1.89.0 (via `rust-toolchain.toml`)
- **Network**: Localnet (dev), Devnet (staging)
- **Version**: 0.18.0
- **Binary**: 495,280 bytes / 3.448 SOL permanent rent
- **Upgrade authority**: Squads V4 2-of-3 multisig PDA `BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC` (OPSEC-002 — see "Deploying an upgrade" below). `anchor deploy` no longer works for upgrades; the first deploy of a fresh program ID still uses `solana program deploy` from Alex Bot, then `set-upgrade-authority` to the Squads vault.

### v0.16 architectural shift (vs v0.15.x)

- **Custodial vault balance is gone.** USDC + USDT live in each user's own ATA — server-signed self-custody for managed-wallet users (Rails holds the encrypted keypair), real self-custody for Phantom users. No more `deposit` / `withdraw` instructions; no more `UserAccount.balance` field.
- **Currency registry on `VaultState`.** `accepted_currencies: [AcceptedCurrency; 16]` is the on-chain source of truth for which mints are accepted as entry fees. USDC at slot 0 (also `payout_mint`), USDT at slot 1, slots 2-15 populated via the `register_currency` 2-of-3 instruction. Adding a new currency = data update, no contract upgrade.
- **`VaultState` is zero-copy.** Borsh's `deserialize_reader` for a 1.5 KB struct exceeds BPF's 4 KB stack frame; `#[account(zero_copy(unsafe))]` + `#[repr(C)]` maps the bytes directly via `bytemuck::Pod`. `paused: u8` and `AcceptedCurrency.active: u8` instead of bool for Pod safety.
- **Prize pool decoupled from entry fees.** `Contest.prize_pool` is creator-funded at create time (USDC, immutable). Entry fees flow to per-currency operator-revenue PDAs (`[b"op_rev", mint]`). Settlement cap is `total_payouts ≤ prize_pool`; entry fees are operator margin.
- **`UserAccount` carries lifetime stat counters** — `entries`, `wins` (rank == 1), `cashes` (rank > 0), `total_won`, plus `seeds` and `username`. No financial state.

## File Layout

```
programs/turf_vault/src/
├── lib.rs              # Program entry — 18 thin wrappers (16 + pause/unpause)
├── state.rs            # 6 account structs + 2 enums + multisig helpers
├── errors.rs           # 6000-6033 (some retired-but-kept for numbering stability)
└── instructions/
    ├── mod.rs                  # Re-exports all instruction modules
    ├── initialize.rs           # Vault setup + register USDC + USDT in slots 0/1
    ├── register_currency.rs    # NEW (2-of-3) — add a currency to the registry
    ├── deactivate_currency.rs  # NEW (2-of-3) — flip a slot's active flag off
    ├── create_user_account.rs  # PDA per wallet: username + stats + seeds
    ├── set_username.rs         # Owner signs; v0.15.1 on-chain validation
    ├── create_season.rs        # Immutable per-entry seed-award schedule
    ├── create_contest.rs       # entry_fee_by_currency[16] + USDC prize_pool
    ├── set_contest_lock_time.rs       # NEW (1-of-3) — set Contest.lock_timestamp (derived time-lock)
    ├── set_contest_conclusion_time.rs # NEW (1-of-3) — set Contest.conclusion_timestamp (finalizes lock)
    ├── enter_contest.rs        # Unified entry (user signs + 1-of-3 payer)
    ├── enter_contest_with_token.rs # Token-funded entry (wallet co-signs OPSEC-004)
    ├── settle_contest.rs       # 2-of-3; SPL CPI per winner from prize_pool PDA
    ├── cancel_contest.rs       # NEW (2-of-3) — refund prize pool to creator
    ├── close_contest.rs        # 1-of-3; sweeps dust + closes the contest PDA
    ├── mint_entry_token.rs     # Admin mints a pre-purchased EntryTokenAccount
    ├── sweep_operator_revenue.rs # NEW (2-of-3) — drain op-rev ATA to treasury
    └── pause.rs / unpause.rs   # Emergency stop (2-of-3 each)
docs/
├── v0.16-spec.md       # Full v0.16 design doc (Jasper-authored, 1,500+ lines)
tests/
└── turf_vault.ts       # v0.15.1 test suite — REWRITE PENDING for v0.16 surface
Anchor.toml             # Program ID, cluster config, test script
scripts/squad.json      # Squads vault PDAs + member pubkeys
scripts/squad-upgrade.js # Upgrade flow via Squad (propose → approve x2 → execute)
```

Files removed in v0.16: `deposit.rs`, `withdraw.rs`, `enter_contest_direct.rs`, `enter_contest_direct_with_token.rs`, `force_close_vault.rs`, `update_signers.rs`.

## 2-of-3 Multisig System

VaultState stores `signers: [Pubkey; 3]` and `threshold: u8`. Two authorization levels:

```rust
// Any 1-of-3 for routine ops (create contest, close contest, enter, mint token)
pub fn is_signer(&self, key: &Pubkey) -> bool {
    self.signers.contains(key)
}

// 2-of-3 for treasury ops (settle, force_close, update_signers)
pub fn validate_multisig(&self, s1: &Pubkey, s2: &Pubkey) -> bool {
    s1 != s2 && self.is_signer(s1) && self.is_signer(s2)
}
```

### Signers
| # | Role | Address |
|---|------|---------|
| 1 | Alex Bot (server) | `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ` |
| 2 | Alex (human) | `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr` |
| 3 | Mason | `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR` |

### Authorization by Instruction (v0.16)
| Instruction | Auth Level | Notes |
|-------------|-----------|-------|
| `initialize` | INIT_AUTHORITY | One-time. Mainnet builds pin to Alex's Phantom key as a compile-time constant. |
| `create_user_account` | Permissionless payer | Anyone can pay rent for any wallet's UserAccount PDA. Username required (≥ 3 chars). |
| `set_username` | User signs | Wallet owner signs; v0.15.1 on-chain validation (reserved prefixes, ASCII, ≥3 chars). |
| `create_season` | 1-of-3 | Any signer can create. Seed schedule immutable after create. |
| `create_contest` | 1-of-3 + creator | Admin pays SOL rent; creator (Phantom or server-keypair) signs the USDC prize-pool transfer. |
| `set_contest_lock_time` | 1-of-3 | Sets `Contest.lock_timestamp` (derived time-lock). Rejected once `conclusion_timestamp` has passed (`ContestConcluded` 6035). |
| `set_contest_conclusion_time` | 1-of-3 | Sets `Contest.conclusion_timestamp`; once passed, the lock time is final. |
| `enter_contest` | User + 1-of-3 payer | User signs SPL transfer from their ATA → currency's op-rev ATA. Admin pays SOL rent (OPSEC-024 channel discipline). |
| `enter_contest_with_token` | User + 1-of-3 payer | Wallet must sign (OPSEC-004) to consent to token consume. |
| `settle_contest` | **2-of-3** | SPL CPI per winner from `[b"prize_pool", contest_id]` PDA. Rails must set `set_compute_unit_limit(400_000)` — default budget tops out near ~25 winners. |
| `cancel_contest` | **2-of-3** | Status → Cancelled. Refunds full `prize_pool` to creator. Entry fees stay with operator (treated as revenue); operator-side playbook is to `mint_entry_token` goodwill credits to affected entrants. |
| `close_contest` | 1-of-3 | Settled or Cancelled contests only. Sweeps any prize_pool dust to op-rev USDC, then closes the contest PDA (rent → admin). |
| `mint_entry_token` | 1-of-3 | Admin mints free-entry voucher PDAs. Idempotent per `source_ref`. |
| `register_currency` | **2-of-3** | Add a mint to `accepted_currencies` at the first empty slot. Also creates the per-currency op-rev ATA at `[b"op_rev", mint]`. |
| `deactivate_currency` | **2-of-3** | Flips `accepted_currencies[idx].active = 0`. Slot is never reclaimed — preserves `currency_idx` stability across the program's life. |
| `sweep_operator_revenue` | **2-of-3** | Drains a currency's op-rev ATA to `vault_state.treasury_authority` (pinned to the Squads vault PDA at init). |
| `pause` / `unpause` | **2-of-3** | Emergency stop. Blocks `enter_contest` + `enter_contest_with_token`. Other ops continue. |

### Co-signing Flow (Treasury Operations)
1. Server (Alex Bot) builds TX and partially signs as `admin`
2. PendingTransaction record created in Rails with serialized TX
3. Human (Alex or Mason) opens Treasury admin page, connects Phantom
4. Phantom signs as `cosigner`, submits to Solana
5. Rails records TX signature and marks operation complete

## Anchor Patterns

### PDA Derivation

| Account | Seeds | Notes |
|---------|-------|-------|
| VaultState | `[b"vault"]` | Singleton |
| UserAccount | `[b"user", wallet]` | One per wallet |
| Contest | `[b"contest", contest_id]` | contest_id = SHA256 of Rails slug |
| ContestEntry | `[b"entry", contest_id, wallet, entry_num.to_le_bytes()]` | Multiple per user |
| EntryTokenAccount | `[b"entry_token", owner, sequence.to_le_bytes()]` | sequence = u64 LE; discover via getProgramAccounts on `owner` |
| Season | `[b"season", season_id.to_le_bytes()]` | season_id = u32 LE |

### CPI Token Transfers

- **Deposit** (user → vault): Standard CPI `transfer` with user as signer
- **Withdraw** (vault → user): CPI `transfer` with PDA seeds `[b"vault", &[bump]]` as signer

### remaining_accounts Pattern (settle_contest)

Settlement passes entry data as `remaining_accounts` — pairs of `[user_account, contest_entry]`. Each pair is:
1. PDA-verified against expected seeds
2. Manually deserialized (`try_deserialize`)
3. Mutated (balance, rank, payout, status)
4. Manually serialized back (`try_serialize`)

This pattern avoids Anchor's account resolution limits for variable-length settlement arrays.

### Account Sizing

All accounts use `#[derive(InitSpace)]`. Contest has `#[max_len(10)]` on `payout_amounts: Vec<u64>` (max 10 payout tiers).

## State Model

### Enums
- `ContestStatus`: Open → Settled OR Open → Cancelled (Cancelled is terminal). `Locked` is now a **vestigial variant** (kept for discriminant stability; no instruction sets it — locking is a derived time-lock, see Key Design Decisions).
- `EntryStatus`: Active → Won (rank > 0, payout > 0) / Lost

### VaultState — zero-copy, `#[account(zero_copy(unsafe))]`, `#[repr(C)]`, 1515 bytes
- `signers: [Pubkey; 3]`, `threshold: u8`, `bump: u8`, `paused: u8`
- `payout_mint: Pubkey` (USDC, pinned at init; immutable)
- `treasury_authority: Pubkey` (Squads vault PDA, pinned at init)
- `accepted_currencies: [AcceptedCurrency; 16]` (1280 bytes — `{ mint, op_rev_ata, kind, active, _pad: [u8; 14] }` × 16)
- `_reserved: [u8; 64]` (forward-compat)

### UserAccount Fields (no balance — v0.16 stat counters)
- `wallet: Pubkey`, `username: [u8; 32]`, `seeds: u64`
- `entries: u32` (lifetime contests entered), `wins: u32` (rank == 1), `cashes: u32` (rank > 0), `total_won: u64` (lifetime USDC payouts)
- `bump: u8`, `_reserved: [u8; 32]`

### Contest Fields
- `contest_id: [u8; 32]`, `admin: Pubkey`, `creator: Pubkey`, `season_id: u32`
- `prize_pool: u64` (USDC, immutable after create)
- `entry_fee_by_currency: [u64; 16]` (parallel to vault_state.accepted_currencies)
- `entry_fees: [u64; 16]` (collected fees per currency — informational, NOT used for settlement cap)
- `max_entries: u32`, `current_entries: u32`, `status: ContestStatus`
- `payout_amounts: Vec<u64>` (max 10 ranks; sum must equal `prize_pool`)
- `lock_timestamp: i64` (v0.17 — derived time-lock; `enter_contest{,_with_token}` reject once `Clock.unix_timestamp >= lock_timestamp`; 0 = no lock)
- `conclusion_timestamp: i64` (v0.18 — once passed, the lock time is final; 0 = no conclusion)
- `bump: u8`, `_reserved: [u8; 16]`

> **Account size UNCHANGED across v0.16→v0.17→v0.18.** `lock_timestamp` (8 bytes) and `conclusion_timestamp` (8 bytes) were carved out of the original `[u8; 32]` `_reserved` (now `[u8; 16]`), so existing Contest PDAs need no re-init — zeroed reserved bytes decode as 0 = no lock / no conclusion.

### Key Constraints
- All token amounts: `u64` with 6 decimals (1 USDC = 1_000_000)
- `payout_amounts.sum() == prize_pool` (validated in `create_contest`)
- `create_contest` rejects when ALL `entry_fee_by_currency` slots are zero AND `prize_pool == 0` (FeeAndPrizeBothZero)
- Settlement total payouts must be `≤ prize_pool` (NOT entry_fees — they're operator revenue now)
- All arithmetic uses `checked_add`/`checked_sub`

## Error Codes

| Code | Name | When |
|------|------|------|
| 6000 | Unauthorized | Non-signer tries an admin action |
| 6001 | InvalidMint | Deposit/withdraw with wrong mint |
| 6002 | InsufficientBalance | Withdraw/enter exceeds balance |
| 6003 | ContestNotOpen | Enter non-open contest |
| 6004 | ContestFull | Contest at max_entries |
| 6005 | ContestNotSettled | Close unsettled contest |
| 6006 | ContestAlreadySettled | Settle already-settled contest, or settle a non-Active entry |
| 6007 | DuplicateEntry | Same entry_num (PDA collision), or duplicate `(wallet, entry_num)` in a settlement |
| 6008 | SettlementOverflow | Payouts > entry_fees + prizes |
| 6009 | Overflow | Arithmetic overflow |
| 6010 | InvalidPayoutTiers | `payout_amounts` does not sum to `prizes` |
| 6011 | AccountAlreadyMigrated | Used by `force_close_vault` (kept for numbering stability; `migrate_user_account` removed in v0.15.1) |
| 6012 | InvalidAccountData | Reserved (was used by `migrate_user_account`, kept for numbering stability) |
| 6013 | InvalidThreshold | Threshold must be 1-3 |
| 6014 | DuplicateSigner | Duplicate signer in array |
| 6015 | EntryTokenAlreadyConsumed | Redeeming an already-consumed entry token |
| 6016 | EntryTokenWrongOwner | Entry token owner ≠ the wallet entering |
| 6017 | SignerContinuityRequired | `update_signers` drops all current cosigners |
| 6018 | VaultPaused | Funds-touching op called while vault is paused (v0.15.0) |
| 6019 | WithdrawDailyCapExceeded | RETIRED in v0.16 (no withdraw instruction); slot kept for numbering stability |
| 6020 | UsernameReserved | Username uses a reserved prefix — admin, system, turf, vault, support, etc. (v0.15.1, audit C2) |
| 6021 | UsernameInvalidChars | Username has bytes outside printable ASCII 0x20..0x7E (v0.15.1, audit C2) |
| 6022 | UsernameTooShort | Username has fewer than 3 non-null bytes (v0.15.1, audit C2) |
| 6023 | CurrencyAlreadyRegistered | `register_currency` called for a mint that's already in the registry |
| 6024 | CurrencyRegistryFull | All 16 slots in `accepted_currencies` are populated |
| 6025 | InvalidCurrencyIndex | `currency_idx` is ≥ 16 OR points to an empty (mint == default) slot |
| 6026 | CurrencyNotActive | Slot is registered but `active == 0` |
| 6027 | EntryFeeNotSet | `contest.entry_fee_by_currency[currency_idx] == 0` |
| 6028 | ContestNotLocked | UNUSED-but-kept (numbering stability) — `unlock_contest` retired in v0.17 |
| 6029 | ContestNotCancellable | `cancel_contest` called against a Settled or already-Cancelled contest |
| 6030 | PrizePoolNotEmpty | `close_contest` finds residual USDC in the prize pool (should be unreachable — close_contest sweeps dust first) |
| 6031 | EmptyRevenueAccount | `sweep_operator_revenue` called against an empty op-rev ATA |
| 6032 | TreasuryAuthorityMismatch | `sweep_operator_revenue` destination ATA owner ≠ `vault_state.treasury_authority` |
| 6033 | FeeAndPrizeBothZero | `create_contest` with `prize_pool == 0` AND all `entry_fee_by_currency` slots zero |
| 6034 | ContestLocked | `enter_contest{,_with_token}` when `Clock.unix_timestamp >= lock_timestamp` (v0.17) |
| 6035 | ContestConcluded | `set_contest_lock_time` rejected once `conclusion_timestamp` has passed (v0.18) |

## Testing

### Run Tests
```bash
anchor test
```

**The v0.15.1 test suite at `tests/turf_vault.ts` will NOT pass against v0.16** — it references `deposit`, `withdraw`, `enter_contest_direct`, etc. that no longer exist, and the account layouts changed. A v0.16 rewrite is pending. Until then, run `cargo check` and `anchor idl build` for static verification; functional coverage lives in turf-monster's Rails + Playwright suites (which DO target v0.16's surface).

### Test Setup Pattern
```typescript
// Initialize with 3 signers, threshold 2
await program.methods
  .initialize([admin.publicKey, signer2.publicKey, signer3.publicKey], 2)
  .accountsStrict({ ... })
  .rpc();

// Verify signers and threshold
const vault = await program.account.vaultState.fetch(vaultStatePda);
expect(vault.signers[0].toBase58()).to.equal(admin.publicKey.toBase58());
expect(vault.threshold).to.equal(2);
```

## Prerequisites

- **Rust**: 1.89.0 (via `rust-toolchain.toml`)
- **Anchor CLI**: 0.32.1 — `/Users/alex/.cargo/bin/anchor`
- **Solana CLI**: `/Users/alex/.local/share/solana/install/active_release/bin/solana`
- **Node.js + Yarn**: Required for TypeScript tests
- **Helius RPC** (or equivalent paid endpoint) for any non-trivial dev work. Public `api.devnet.solana.com` rate-limits `getProgramAccounts` aggressively (~1 req/sec/IP) which breaks `Solana::Vault.list_entry_tokens` and `solana program show`. Set `SOLANA_RPC_URL=https://devnet.helius-rpc.com/?api-key=...` in your shell or in the consuming app's `.env`. Helius URLs live in 1Password at `agent.helius` (Devnet + Mainnet on the same key).

## Build & Deploy

```bash
# Build
anchor build

# Test (starts local validator automatically)
anchor test
```

### Deploying an upgrade (OPSEC-002 — Squads multisig)

**`anchor deploy` no longer works.** As of 2026-05-19 the program upgrade
authority is a Squads V4 2-of-3 multisig (Alex Bot / Alex / Mason), not a
single keypair. Every upgrade goes through the Squad. Config + the reusable
tool live in `scripts/`:

```bash
# 1. Build
anchor build

# 2. Write the new program binary to a buffer account
solana program write-buffer target/deploy/turf_vault.so --url devnet
#    → prints "Buffer: <BUFFER_ADDR>"

# 3. Hand the buffer to the Squad vault (buffer authority must match the
#    program's upgrade authority for the upgrade instruction to accept it)
solana program set-buffer-authority <BUFFER_ADDR> \
  --new-buffer-authority BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC --url devnet

# 4. Run the Squad upgrade. The script auto-extends the ProgramData account
#    first if the new binary is larger (the bare BPF `upgrade` instruction
#    doesn't grow it the way `solana program deploy` does), then wraps the
#    `upgrade` in a Squad vault transaction → propose → approve (Alex Bot)
#    → approve (Mason) → execute. Keys from 1Password.
ALEX_BOT_KEY=$(op item get agent.solana       --vault agents --fields "private key" --reveal) \
MASON_KEY=$(op item get agent.mason.solana    --vault agents --fields "private key" --reveal) \
  node scripts/squad-upgrade.js <BUFFER_ADDR>

# 5. Verify
solana program show Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT --url devnet
#    → "Last Deployed In Slot" should be recent
```

Any 2 of the 3 members can approve. `scripts/squad-upgrade.js` uses Alex Bot
+ Mason because their keys are in 1Password; to cosign with Alex's Phantom
instead, use the Squads app (https://app.squads.so, devnet). Squad addresses
are in `scripts/squad.json`. Full migration story: `docs/agents/system/
squads-upgrade-authority-migration.md` in mcritchie-studio.

```bash
# Verify current state
solana program show Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT --url devnet
```

### `anchor test` Workaround

Anchor CLI 0.32.1 can't find `node`/`yarn` in PATH due to Rust subprocess spawning. If `anchor test` fails to find node, deploy manually then run tests directly:

```bash
ANCHOR_PROVIDER_URL=http://127.0.0.1:8899 ANCHOR_WALLET=~/.config/solana/id.json yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/**/*.ts
```

### Devnet SOL Faucet Protocol

Follow this sequence when SOL is needed. Move to the next step only if the current one fails.

| Step | Method | Command / URL | Notes |
|------|--------|---------------|-------|
| 1 | **PoW faucet** | `devnet-pow mine --target-lamports <amount> -ud` | Preferred. Consistent, no rate limits. Install: `cargo install devnet-pow`. If public RPC times out, pass `-u <rpc_url>` with a provider endpoint. |
| 2 | **QuickNode faucet** | https://faucet.quicknode.com/solana/devnet | Web UI, no account required. Paste wallet address. |
| 3 | **Solana Foundation faucet** | https://faucet.solana.com | Web UI. Select Devnet, paste address. |
| 4 | **CLI airdrop** | `solana airdrop <amount> --url devnet` | Last resort — frequently rate-limited. Try smaller amounts (0.5 SOL). |
| 5 | **Transfer from funded wallet** | `solana transfer <to> <amount> --url devnet` | If another project wallet has spare SOL. |

**Never rely solely on `solana airdrop`** — it rate-limits aggressively and fails silently under devnet load.

### Migration (Re-initialization)

When the VaultState schema changes (e.g. adding a field to `signers` handling), the old account must be closed first:

```bash
# From Rails app:
bin/rails solana:init_vault FORCE_CLOSE=true
bin/rails solana:init_vault INIT=true SIGNERS=addr1,addr2,addr3 THRESHOLD=2
```

### Current Deployment (Devnet)

- **Program ID**: `EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ` (the orphaned `Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT` is dead — see header)
- **Upgrade authority**: Squads V4 vault PDA `BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC` (2-of-3 multisig — OPSEC-002)
- **VaultState PDA**: derived `[b"vault"]` under the EQGF program (the old `FYBTB5pwoSxN…vpWAn` was Dx8u-era — refresh from `bin/rails runner` if you need the literal)
- **Signer 1**: Alex Bot — `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ`
- **Signer 2**: Alex — `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`
- **Signer 3**: Mason — `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR`
- **Threshold**: 2-of-3 for treasury ops
- **USDC Mint**: `222Dcu2RgAXE3T8A4mGSG3kQyXaNjqePx7vva1RdWBN9` (test, 6 decimals)
- **USDT Mint**: `9mxkN8KaVA8FFgDE2LEsn2UbYLPG8Xg9bf4V9MYYi8Ne` (test, 6 decimals)

**Status**: **v0.18.0 deployed on devnet 2026-05-31 (slot 465782911)** via the Squads upgrade (v0.17.0 was slot 465778752). v0.17 added the derived on-chain time-lock — `Contest.lock_timestamp` (carved from `_reserved`, no size change) + `set_contest_lock_time` (1-of-3); `enter_contest{,_with_token}` reject once `Clock.unix_timestamp` passes it (`ContestLocked` 6034); retired `lock_contest`/`unlock_contest`. v0.18 adds `Contest.conclusion_timestamp` (also carved from `_reserved`, no size change) + `set_contest_conclusion_time` (1-of-3); once passed, the lock time is final (`set_contest_lock_time` then rejects with `ContestConcluded` 6035). IDL hash (re-pinned in turf-monster): `2d87b0935f5cd217b04a98153033c371d0b6f90018e9713acf3c3b44fe4db263`. 2-of-3 multisig for treasury ops; upgrade authority is the Squads V4 vault. Vault initialized with 3 signers (Alex Bot, Alex, Mason), threshold 2.

> **Note**: the program was migrated off the orphaned ID `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J` on 2026-05-18 (its upgrade authority was lost). ~3.45 SOL of rent stays locked at the old program forever (devnet only).

## Versioning Protocol

- **Semantic versioning** in `programs/turf_vault/Cargo.toml`
  - MAJOR: Breaking account layout changes
  - MINOR: New instructions or features
  - PATCH: Bug fixes, validation improvements
- **After each deploy**:
  1. Bump version in Cargo.toml
  2. Update CHANGELOG.md
  3. Commit: `git commit -m "v0.X.Y: description"`
  4. Tag: `git tag -a v0.X.Y -m "description"`
  5. Push: `git push origin main --tags`
  6. **Re-pin the IDL hash in turf-monster** (OPSEC-014). turf-monster commits `config/turf_vault.idl.json` and refuses to boot — and to precompile assets — in production when its SHA256 ≠ `EXPECTED_IDL_HASH`. Do not skip — running prod against a drifted IDL silently corrupts every Borsh decode.

     Use the freshly-**built** IDL, NOT `anchor idl fetch`: a Squad upgrade runs only the BPF `upgrade` instruction and does NOT update the on-chain IDL account, so `anchor idl fetch` returns the stale pre-upgrade IDL.
     ```bash
     cp target/idl/turf_vault.json /Users/alex/projects/turf-monster/config/turf_vault.idl.json
     cd /Users/alex/projects/turf-monster
     shasum -a 256 config/turf_vault.idl.json   # = EXPECTED_IDL_HASH
     # Set EXPECTED_IDL_HASH on Heroku BEFORE `git push heroku main` (the build's
     # assets:precompile runs verify_idl!), then commit the IDL JSON + deploy.
     ```

## Integration with Turf Monster

The Rails app calls TurfVault through a `Solana::Vault` service layer:

- **Contest ID**: SHA256 hash of the contest slug (e.g. `"turf-totals-v1-matchday-1"` → 32-byte array)
- **Entry num**: Sequential integer per user per contest
- **Settlement**: Rails grades contest → builds settlement array → calls settle_contest
- **Token amounts**: Rails stores cents (integer), Solana stores 6-decimal u64. Convert: `amount_cents * 10_000` (cents → 6 decimals)
- **Admin key**: `SOLANA_ADMIN_KEY` env var (base58 private key of Alex Bot)

## Key Design Decisions

- **Managed entry** (`enter_contest`): Separates payer (admin signer) from wallet (entry owner) — deducts from UserAccount PDA balance. For server-managed wallets.
- **Direct entry** (`enter_contest_direct`): User signs USDC transfer from their own wallet ATA to vault. Admin pays PDA rent so user only spends USDC. For Phantom wallets. Added in v0.3.0. Requires `user_account` PDA (for seeds award) since v0.5.0.
- **Hard escrow contest creation** (`create_contest` v0.4.0): Dual-signer — `payer` (admin bot, pays SOL rent) + `creator` (Phantom wallet, signs prizes USDC transfer from creator ATA → vault). Contest struct stores `creator` pubkey. If prizes is 0, no transfer occurs but token accounts are still required.
- **Derived time-lock** (v0.17/v0.18): Locking is NOT a status transition — it's a derived time-lock. `set_contest_lock_time` (1-of-3) sets `Contest.lock_timestamp`; `enter_contest{,_with_token}` reject once `Clock.unix_timestamp >= lock_timestamp` (`ContestLocked` 6034). `set_contest_conclusion_time` (1-of-3) sets `conclusion_timestamp`; once passed, the lock time is final (`set_contest_lock_time` then rejects with `ContestConcluded` 6035). The old `lock_contest`/`unlock_contest` status instructions are retired; `ContestStatus::Locked` survives only as a vestigial discriminant.
- **2-of-3 Multisig** (v0.8.0): Treasury ops require 2 distinct signers (`admin` + `cosigner`). Routine ops require any 1-of-3. Server partially signs, human cosigns via Phantom.
- **Dual mint**: USDC + USDT supported from day one, separate vault token accounts
- **Seeds system** (v0.5.0, per-season schedule since v0.11.0): All four entry instructions (`enter_contest`, `enter_contest_direct`, `enter_contest_with_token`, `enter_contest_direct_with_token`) award seeds to the user's `UserAccount` PDA from the contest's bound `Season` — `season.seed_schedule[entry_num.min(4)]` (entries 5+ clamp to slot 4). Seeds are on-chain only — Rails reads them via `sync_balance` and derives levels in the UI (`level = seeds / 100 + 1`).
- **Manual settlement**: No on-chain scoring — Rails computes results, admin submits final rankings
- **force_close_vault**: Migration instruction that reads signers from raw bytes (avoids deserialization of old schema). Requires 2-of-3 cosign.
- **update_signers** (v0.8.0): Rotate signers or change threshold. Requires 2-of-3 cosign.

## Code Style

- Keep instructions in separate files under `instructions/`
- Thin wrappers in `lib.rs`, logic in instruction handlers
- All state in `state.rs`, all errors in `errors.rs`
- Use `msg!()` for on-chain logging at key events
- Prefer `require!()` macro over manual `if/return Err`
