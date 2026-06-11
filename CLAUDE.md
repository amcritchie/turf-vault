# TurfVault — Development Instructions

## Project Overview

Anchor smart contract for contest escrow on Solana. Backend for Turf Monster (Rails pick'em app).

- **Program ID (devnet)**: `EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ` — fresh deploy 2026-05-27 (the prior `Dx8u…GaCT` is orphaned with ~4 SOL ProgramData rent locked under Squads authority).
- **Framework**: Anchor 0.32.1
- **Rust**: 1.89.0 (via `rust-toolchain.toml`)
- **Network**: Localnet (dev), Devnet (staging)
- **Version**: 0.25.0 on this branch (admin username flows) — **DEPLOYED to devnet 2026-06-11 (slot 468716417, Squads 2-of-3: 8K81 + Mason) and shaken down on-chain** (admin_create_user_account with reserved "turf" ✓; plain path still rejects 6020 ✓; admin_set_username ✓). **Mainnet stays v0.24** until the next upgrade window. `main` is caught up to **v0.24.0**, the version deployed on mainnet (`DaFv83yo…`, 2026-06-08) — grant_seeds admin quest grants + Season.quest_seeds; v0.21 name/slug dropped; see CHANGELOG.
- **Binary**: 531,792 bytes (.so) / ProgramData 532,184 bytes (~3.7 SOL devnet rent) — v0.24 build
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
├── lib.rs              # Program entry — 21 thin wrappers (v0.25 added the admin username flows)
├── state.rs            # 6 account structs + 2 enums + multisig helpers
├── errors.rs           # 6000-6044 (6039-41 name/slug, 6042-44 grant_seeds; some retired-but-kept)
└── instructions/
    ├── mod.rs                  # Re-exports all instruction modules
    ├── initialize.rs           # Vault setup + register USDC + USDT in slots 0/1
    ├── update_signers.rs       # RE-ADDED v0.20 (2-of-3) — rotate signers/threshold in place
    ├── register_currency.rs    # NEW (2-of-3) — add a currency to the registry
    ├── deactivate_currency.rs  # NEW (2-of-3) — flip a slot's active flag off
    ├── create_user_account.rs  # PDA per wallet: username + stats + seeds
    ├── set_username.rs         # Owner signs; v0.15.1 on-chain validation
    ├── admin_create_user_account.rs # NEW v0.25 — create + 1-of-3 co-sign waives reserved-prefix only
    ├── admin_set_username.rs   # NEW v0.25 — owner signs + 1-of-3 co-sign waives reserved-prefix only
    ├── create_season.rs        # Per-entry seed_schedule + v0.23 quest_seeds[16] (immutable)
    ├── grant_seeds.rs          # v0.22 (1-of-3) — standalone admin quest-bonus grant; flexible kinds 0-15 (v0.23)
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

> **`update_signers.rs` RE-ADDED in v0.20** (adapted to zero-copy VaultState). Removing it in v0.16 made the deployed signer set immutable — a leaked signer key forces a full redeploy instead of an on-chain rotation. v0.20 brings it back (2-of-3, signer-continuity guarded) so future compromise is a cheap on-chain tx. The redeploy that ships v0.20 (forced by the current Alex Bot key leak) is documented in `docs/KEY_ROTATION.md`.

## 2-of-3 Multisig System

VaultState stores `signers: [Pubkey; 3]` and `threshold: u8`. Two authorization levels:

```rust
// Any 1-of-3 for routine ops (create contest, close contest, enter, mint token)
pub fn is_signer(&self, key: &Pubkey) -> bool {
    self.signers.contains(key)
}

// 2-of-3 for treasury + governance ops (settle, cancel, sweep, pause/unpause,
// register/deactivate_currency, update_signers)
pub fn validate_multisig(&self, s1: &Pubkey, s2: &Pubkey) -> bool {
    s1 != s2 && self.is_signer(s1) && self.is_signer(s2)
}
```

### Signers
| # | Role | Address |
|---|------|---------|
| 1 | Alex Bot (server) | `8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd` |
| 2 | Alex (human) | `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr` |
| 3 | Mason | `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR` |

> **Key rotation (2026-06-06):** the leaked Alex Bot key `F6f8…KzhZ` is retired from BOTH the VaultState signer set AND the Squads upgrade-authority members, replaced by `8K81…RaRYd` (1Password `agent.alex.solana`). Devnet Squads members are now `8K81 / 7ZDJ / CytJ`. F6f8 has zero authority on devnet; `docs/KEY_ROTATION.md` is superseded.

### Authorization by Instruction (v0.16)
| Instruction | Auth Level | Notes |
|-------------|-----------|-------|
| `initialize` | INIT_AUTHORITY | One-time. Mainnet builds pin to Alex's Phantom key as a compile-time constant. |
| `update_signers` | **2-of-3** | Rotate signer pubkeys in place (no redeploy). Threshold is PINNED at 2-of-3 — this rotates signers only (`validate_multisig` ignores the threshold field). `validate_multisig(admin, cosigner)`. Continuity-guarded: no duplicate signers (6014), no default/zeroed slots + BOTH authorizing cosigners survive the rotation (`SignerContinuityRequired` 6017). Re-added v0.20 (adapted to zero-copy VaultState). |
| `create_user_account` | Permissionless payer | Anyone can pay rent for any wallet's UserAccount PDA. Username required (≥ 3 chars). |
| `set_username` | User signs | Wallet owner signs; v0.15.1 on-chain validation (reserved prefixes, ASCII, ≥3 chars). |
| `admin_create_user_account` | Permissionless payer + **1-of-3 co-sign** | v0.25. Same as `create_user_account` plus a required `admin: Signer` validated `vault_state.is_signer` (else `Unauthorized` 6000). Waives ONLY the reserved-prefix check — charset + ≥3 chars still enforced. Adds `vault_state` (read-only; the plain variant doesn't load it). For the operator house account ("turf"). |
| `admin_set_username` | User + **1-of-3 co-sign** | v0.25. Same as `set_username` (owner signs, consenting) plus the required vault-signer `admin` co-signer; same prefix-only waiver. |
| `create_season` | 1-of-3 | Any signer can create. Seed schedule immutable after create. |
| `create_contest` | 1-of-3 + creator | Admin pays SOL rent; creator (Phantom or server-keypair) signs the USDC prize-pool transfer. |
| `set_contest_lock_time` | 1-of-3 / **2-of-3 post-lock** | Sets `Contest.lock_timestamp`. Pre-lock: 1-of-3. Amending a lock that has ALREADY PASSED requires 2-of-3 (optional `cosigner`) — closes the #5 re-open vector. Rejected once concluded (`ContestConcluded` 6035) or on invalid timestamps (`InvalidTimestamp` 6037). |
| `set_contest_conclusion_time` | 1-of-3 / **2-of-3 to amend** | First set (0→value): 1-of-3. Amending an already-set conclusion: 2-of-3 (optional `cosigner`). Must be in the future and after a set lock (`InvalidTimestamp` 6037). |
| `enter_contest` | User + 1-of-3 payer | User signs SPL transfer from their ATA → currency's op-rev ATA. Admin pays SOL rent (OPSEC-024 channel discipline). |
| `enter_contest_with_token` | User + 1-of-3 payer | Wallet must sign (OPSEC-004) to consent to token consume. |
| `settle_contest` | **2-of-3** | SPL CPI per winner from `[b"prize_pool", contest_id]` PDA → each winner's canonical ATA, bound on-chain (`InvalidPayoutDestination` 6036, #3). Requires the lock OR conclusion timestamp to have passed (`ContestNotLocked` 6028, #6). Rails must set `set_compute_unit_limit(400_000)` — default budget tops out near ~25 winners. |
| `cancel_contest` | **2-of-3** | Status → Cancelled. Refunds full `prize_pool` to creator. Entry fees stay with operator (treated as revenue); operator-side playbook is to `mint_entry_token` goodwill credits to affected entrants. |
| `close_contest` | 1-of-3 | Settled or Cancelled contests only. Sweeps any prize_pool dust to op-rev USDC, then closes the contest PDA (rent → admin). |
| `mint_entry_token` | 1-of-3 | Admin mints free-entry voucher PDAs. Idempotent per `source_ref` — the PDA is seeded on `sha256(source_ref)` (passed as `source_ref_hash`, asserted on-chain, `EntryTokenSeedMismatch` 6038), so re-minting the same ref collides on init (v0.19, #9). `source_ref` must be globally unique across wallets. |
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
| EntryTokenAccount | `[b"entry_token", sha256(source_ref)]` | v0.19 (#9): source_ref hashed to 32 bytes (passed as `source_ref_hash`, asserted on-chain). Discover via getProgramAccounts on `owner` (still in the body). |
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
| 6017 | SignerContinuityRequired | `update_signers` new set drops EITHER authorizing cosigner (both must survive a 2-of-3 rotation), or contains a default/zeroed slot. UN-RETIRED in v0.20 (was reserved-but-unused v0.16–v0.19). |
| 6018 | VaultPaused | Funds-touching op called while vault is paused (v0.15.0) |
| 6019 | WithdrawDailyCapExceeded | RETIRED in v0.16 (no withdraw instruction); slot kept for numbering stability |
| 6020 | UsernameReserved | Username uses a reserved prefix — admin, system, turf, vault, support, etc. (v0.15.1, audit C2). The v0.25 `admin_*` username variants waive ONLY this check via a 1-of-3 vault-signer co-sign. |
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
| 6036 | InvalidPayoutDestination | `settle_contest` winner_ata ≠ the winner's canonical ATA (v0.19, #3) |
| 6037 | InvalidTimestamp | `set_contest_lock_time`/`set_contest_conclusion_time` given a negative/past or mis-ordered (lock ≥ conclusion) timestamp (v0.19, #5) |
| 6038 | EntryTokenSeedMismatch | `mint_entry_token` `source_ref_hash` ≠ sha256(`source_ref`) (v0.19, #9) |

## Testing

### Run Tests
```bash
anchor test
```

**The v0.15.1 test suite at `tests/turf_vault.ts` will NOT pass against v0.16+** — it references `deposit`, `withdraw`, `enter_contest_direct`, etc. that no longer exist, and the account layouts changed (verified still true 2026-06-10: `deposit`/`withdraw` blocks + `account.balance` reads remain). A rewrite for the current surface is pending. Until then, run `cargo check` and `anchor idl build` for static verification; functional coverage lives in turf-monster's Rails + Playwright suites (which DO target the current surface). New per-feature blocks (e.g. `admin username flows (v0.25)`) are written to the current surface and will run once the legacy blocks are rewritten or pruned.

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

> `anchor build` prints several `Error: Function ..crypto_common..hazmat..SerializableState.. Stack offset of N exceeded max offset of 4096` lines. These are dependency-level warnings from the sha2/crypto_common crates (pulled in with the direct `solana-program` dep for `hash()`, v0.19) on functions our program never calls on-chain; the build still completes ("Finished `release` profile") and v0.19+ binaries built this way run fine on devnet + mainnet. Not introduced by your change if you see them.

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
#    NOTE: the rotated Alex Bot key (8K81…) lives at `agent.alex.solana`
#    (the old `agent.solana` item is gone with the leaked F6f8 key).
#    squad-upgrade.js reads ONLY the TOP-LEVEL fields of scripts/squad.json,
#    which on main is the MAINNET config — for a devnet upgrade, temporarily
#    swap in the `_devnet_reference` values (network/programId/multisigPda/
#    vaultPda) and revert after. Never commit the swap.
ALEX_BOT_KEY=$(op item get agent.alex.solana  --vault agents --fields "private key" --reveal) \
MASON_KEY=$(op item get agent.mason.solana    --vault agents --fields "private key" --reveal) \
  node scripts/squad-upgrade.js <BUFFER_ADDR>

# 5. Verify
solana program show EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ --url devnet
#    → "Last Deployed In Slot" should be recent
```

Any 2 of the 3 members can approve. `scripts/squad-upgrade.js` uses Alex Bot
+ Mason because their keys are in 1Password; to cosign with Alex's Phantom
instead, use the Squads app (https://app.squads.so, devnet). Squad addresses
are in `scripts/squad.json`. Full migration story: `docs/agents/system/
squads-upgrade-authority-migration.md` in mcritchie-studio.

```bash
# Verify current state
solana program show EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ --url devnet
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
- **Signer 1**: Alex Bot — `8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd` (rotated from the leaked `F6f8…` on 2026-06-06)
- **Signer 2**: Alex — `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`
- **Signer 3**: Mason — `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR`
- **Threshold**: 2-of-3 for treasury ops
- **USDC Mint**: `222Dcu2RgAXE3T8A4mGSG3kQyXaNjqePx7vva1RdWBN9` (test, 6 decimals)
- **USDT Mint**: `9mxkN8KaVA8FFgDE2LEsn2UbYLPG8Xg9bf4V9MYYi8Ne` (test, 6 decimals)

**Status**: **v0.25.0 deployed on devnet 2026-06-11 (slot 468716417)** via headless Squads 2-of-3 (8K81 Alex Bot + Mason; buffer GwU5U7XG…) — adds `admin_create_user_account` + `admin_set_username` (1-of-3 co-sign waives ONLY the reserved-prefix check). Devnet shakedown same day: admin path created username "turf" ✓, plain path rejected a reserved name with 6020 ✓, admin_set_username ✓. **Mainnet remains v0.24.0** (`DaFv83yokwTz8msP9CzJ13eazSGk15NuUTxjkfzJzxMM`, 2026-06-08; freshly-built mainnet IDL hashes to `5265cc497862ea39d5f3b99bf1fbf42d0cbd51678ca2237b0cc584ee117dde80` = `EXPECTED_IDL_HASH` on turf-monster-mainnet). v0.24 is the consolidated quest-seed release on top of v0.20 — it folds the interim v0.22/v0.23 dev iterations and **DROPS the v0.21 on-chain Contest name/slug** (only its parked error codes 6039-6041 remain; `create_contest` is unchanged from v0.20; the app uses the Rails slug-decouple). Devnet upgrade trail: pre-consolidation **v0.23.0** slot 467696203 (2026-06-06, autonomous Squads upgrade — `8K81` Alex Bot + Mason, 2-of-3) and **v0.22.0** slot 467641352. Feature summary — `grant_seeds` (1-of-3 admin quest-bonus grants; once-ever guard PDA `[b"seed_grant", wallet, kind, invitee]`; errors 6042-6044) accepts any `kind` `0..=15` so new quests need only a Rails constant — never another redeploy (`CHAT_MESSAGE = 3` added); `Season.quest_seeds: [u64; 16]` puts each quest's seed reward on-chain beside the entry `seed_schedule`, tuned by minting a new Season. **Season grew 101 → 229 bytes** — mint fresh, no migration (devnet Season 2 minted, quest_seeds `[25,25,25,25,…]`). **v0.20** re-added `update_signers`. **Key rotation 2026-06-06:** leaked Alex Bot `F6f8…` retired from VaultState signers + Squads members → `8K81…`; devnet members now `8K81 / 7ZDJ / CytJ` (re-verified on-chain against multisig `7nRuVw3VZFC6z85tYVDitPnaUHZCkqLpJRSTBNtPmtZB` 2026-06-11).

Earlier: **v0.19.0 deployed on devnet 2026-06-02 (slot 466341566)** via the Squads upgrade (v0.18.0 was slot 465782911; v0.17.0 was 465778752). Verified by gold-standard compare 2026-06-02: the on-chain program binary (dumped, padding stripped) is byte-for-byte identical to a fresh `anchor build` of this source (HEAD `040ef3e`), and the freshly-built IDL hashes to `99d551001cd69468c8416292e150050c2d6307743c79a0c261211053004992c8` (== the `EXPECTED_IDL_HASH` pinned in turf-monster). **No account-layout / byte-size change from v0.18.** v0.19 is the audit-highs release: (#3) `settle_contest` binds each payout destination to the winner's canonical USDC ATA (`InvalidPayoutDestination` 6036); (#6) `settle_contest` requires the derived lock OR conclusion timestamp to have passed (reuses `ContestNotLocked` 6028); (#5) amending an ALREADY-PASSED `lock_timestamp` requires a distinct 2-of-3 cosigner (1-of-3 attempt → `Unauthorized` 6000), plus timestamp-validity guard (`InvalidTimestamp` 6037); (#9) `mint_entry_token` PDA seed is `source_ref_hash` asserted `== sha256(source_ref)` for true on-chain idempotency (`EntryTokenSeedMismatch` 6038). v0.17 added the derived on-chain time-lock — `Contest.lock_timestamp` (carved from `_reserved`, no size change) + `set_contest_lock_time` (1-of-3); `enter_contest{,_with_token}` reject once `Clock.unix_timestamp` passes it (`ContestLocked` 6034); retired `lock_contest`/`unlock_contest`. v0.18 added `Contest.conclusion_timestamp` (also carved from `_reserved`, no size change) + `set_contest_conclusion_time` (1-of-3); once passed, the lock time is final (`set_contest_lock_time` then rejects with `ContestConcluded` 6035). 2-of-3 multisig for treasury ops; upgrade authority is the Squads V4 vault. Vault initialized with 3 signers (Alex Bot, Alex, Mason), threshold 2.

> **Devnet shakedown (2026-06-02, v0.19)** — all four audit fixes exercised end-to-end on devnet (Alex Bot admin + Mason cosigner, 2-of-3): #6 settle-before-lock rejected `ContestNotLocked` 6028; #3 settle to a non-winner same-mint ATA rejected `InvalidPayoutDestination` 6036; honest 2-of-3 settle after lock paid exactly the prize (5 USDC) prize_pool→winner-ATA and drained the pool (contest → Settled); #5 post-lock lock-time amend rejected 1-of-3 (`Unauthorized` 6000) and accepted 2-of-3. Note: devnet validator Clock lags wall time by tens of seconds — settle the contest a minute past `lock_timestamp`, not on the dot.

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
- **update_signers** (v0.8.0; REMOVED v0.16; RE-ADDED v0.20): Rotate signers IN PLACE. Threshold is PINNED at 2-of-3 — v0.20 rotates signer pubkeys only (the v0.15.x version took a threshold arg, but `validate_multisig` never reads `threshold`, so a configurable threshold was inert + misleading; the arg was dropped). Requires 2-of-3 cosign of the current signers. v0.16 removed the instruction entirely (immutable signer set → leaked key forces redeploy); v0.20 re-adds it adapted to the zero-copy `AccountLoader<VaultState>`. Signer-continuity guard (`SignerContinuityRequired` 6017): the new set keeps BOTH authorizing cosigners and no default/zeroed slot, so a 2-of-3 rotation can't brick the multisig.

## Code Style

- Keep instructions in separate files under `instructions/`
- Thin wrappers in `lib.rs`, logic in instruction handlers
- All state in `state.rs`, all errors in `errors.rs`
- Use `msg!()` for on-chain logging at key events
- Prefer `require!()` macro over manual `if/return Err`
