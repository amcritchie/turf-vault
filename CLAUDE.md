# TurfVault — Development Instructions

## Project Overview

Anchor smart contract for contest escrow on Solana. Backend for Turf Monster (Rails pick'em app).

- **Program ID**: `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J`
- **Framework**: Anchor 0.32.1
- **Rust**: 1.89.0 (via `rust-toolchain.toml`)
- **Network**: Localnet (dev), Devnet (staging)
- **Version**: 0.8.0

## File Layout

```
programs/turf-vault/src/
├── lib.rs              # Program entry — 12 instruction handlers (thin wrappers)
├── state.rs            # 4 account structs + 2 enums + multisig helpers
├── errors.rs           # 13 error codes (VaultError enum)
└── instructions/
    ├── mod.rs           # Re-exports all instruction modules
    ├── initialize.rs    # Vault setup, accepts signers[3] + threshold
    ├── create_user_account.rs
    ├── deposit.rs       # User → vault token transfer via CPI
    ├── withdraw.rs      # Vault → user token transfer via PDA signer
    ├── create_contest.rs
    ├── enter_contest.rs # Debit PDA balance, build prize pool (managed wallets)
    ├── enter_contest_direct.rs # User signs USDC transfer from wallet ATA (Phantom wallets)
    ├── settle_contest.rs # remaining_accounts pattern, requires cosigner (2-of-3)
    ├── close_contest.rs
    ├── force_close_vault.rs # Migration-only: requires cosigner (2-of-3)
    └── update_signers.rs # Update multisig signers/threshold (2-of-3)
tests/
└── turf-vault.ts       # 25 test cases covering all instructions + multisig
Anchor.toml             # Program ID, cluster config, test script
```

## 2-of-3 Multisig System

VaultState stores `signers: [Pubkey; 3]` and `threshold: u8`. Two authorization levels:

```rust
// Any 1-of-3 for routine ops (create contest, close contest, enter, migrate)
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

### Authorization by Instruction
| Instruction | Auth Level | Notes |
|-------------|-----------|-------|
| `create_contest` | 1-of-3 | Any signer can create |
| `close_contest` | 1-of-3 | Any signer can close |
| `enter_contest` / `enter_contest_direct` | 1-of-3 | Any signer can facilitate entries |
| `migrate_user_account` | 1-of-3 | Any signer can migrate |
| `settle_contest` | **2-of-3** | Requires `admin` + `cosigner` |
| `force_close_vault` | **2-of-3** | Requires `admin` + `cosigner` |
| `update_signers` | **2-of-3** | Requires `admin` + `cosigner` |

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
- `ContestStatus`: Open → Locked → Settled
- `EntryStatus`: Active → Won / Lost

### UserAccount Fields
- `wallet`, `balance`, `total_deposited`, `total_withdrawn`, `total_won`, `seeds` (u64, 65 per entry), `bump`

### Contest Fields
- `contest_id`, `prizes`, `entry_fee`, `entry_fees`, `max_entries`, `current_entries`, `status`, `payout_amounts` (Vec, max 10), `admin` (payer pubkey), `creator` (prizes funder pubkey), `bump`

### Key Constraints
- All token amounts: `u64` with 6 decimals (1 USDC = 1_000_000)
- `payout_amounts` sum must equal `prizes` (validated in create_contest)
- Settlement total payouts must be ≤ entry_fees + prizes
- All arithmetic uses `checked_add`/`checked_sub`

## Error Codes

| Code | Name | When |
|------|------|------|
| 6000 | Unauthorized | Non-admin tries admin action |
| 6001 | InvalidMint | Deposit/withdraw with wrong mint |
| 6002 | InsufficientBalance | Withdraw/enter exceeds balance |
| 6003 | ContestNotOpen | Enter non-open contest |
| 6004 | ContestFull | Contest at max_entries |
| 6005 | ContestNotSettled | Close unsettled contest |
| 6006 | ContestAlreadySettled | Settle already-settled contest |
| 6007 | DuplicateEntry | Same entry_num (PDA collision) |
| 6008 | SettlementOverflow | Payouts > entry_fees + prizes |
| 6009 | Overflow | Arithmetic overflow |
| 6010 | InvalidPayoutTiers | Bad payout_amounts config |
| 6011 | InvalidAccountData | Account data parsing failed |
| 6012 | MigrationNotNeeded | Account already at current size |
| 6013 | InvalidThreshold | Threshold must be 1-3 |
| 6014 | DuplicateSigner | Duplicate signer in array |

## Testing

### Run Tests
```bash
anchor test
```

25 tests covering: initialize (with 3 signers + threshold), create_user_account, deposit (USDC/USDT + invalid mint), create_contest (admin with bonus USDC transfer + non-admin rejection), enter_contest (2 users + insufficient balance), settle_contest (payouts with cosigner + already-settled + non-admin + same-signer-twice + non-signer-cosigner), withdraw (success + insufficient balance), close_contest (settled + unsettled), any signer can create contest, update_signers (valid + invalid threshold). Tests use SOL transfers from admin instead of `requestAirdrop` (broken in Solana v3.1).

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

When the VaultState schema changes (e.g. adding `admin_backup`), the old account must be closed first:

```bash
# From Rails app:
bin/rails solana:init_vault FORCE_CLOSE=true
bin/rails solana:init_vault INIT=true SIGNERS=addr1,addr2,addr3 THRESHOLD=2
```

### Current Deployment (Devnet)

- **Program ID**: `7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J`
- **Vault PDA**: `7z313HTVNcxhvCBkkDQv794RpXeRrfCLb5WJ4dFAQQeh`
- **Signer 1**: Alex Bot — `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ`
- **Signer 2**: Alex — `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`
- **Signer 3**: Mason — `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR`
- **Threshold**: 2-of-3 for treasury ops
- **USDC Mint**: `222Dcu2RgAXE3T8A4mGSG3kQyXaNjqePx7vva1RdWBN9` (test, 6 decimals)
- **USDT Mint**: `9mxkN8KaVA8FFgDE2LEsn2UbYLPG8Xg9bf4V9MYYi8Ne` (test, 6 decimals)
- **IDL Account**: `DCP2XRu8ZwzsCpXBgu5xa4vTYdYQhKUZRU49iJuFv8Lf`

**Status**: v0.8.0 deployed on devnet. 2-of-3 multisig for treasury ops. Vault re-initialized with 3 signers (Alex Bot, Alex, Mason), threshold 2.

## Versioning Protocol

- **Semantic versioning** in `programs/turf-vault/Cargo.toml`
  - MAJOR: Breaking account layout changes
  - MINOR: New instructions or features
  - PATCH: Bug fixes, validation improvements
- **After each deploy**:
  1. Bump version in Cargo.toml
  2. Update CHANGELOG.md
  3. Commit: `git commit -m "v0.X.Y: description"`
  4. Tag: `git tag -a v0.X.Y -m "description"`
  5. Push: `git push origin main --tags`
  6. **Re-pin the IDL hash in turf-monster** (OPSEC-014). The on-chain IDL is overwritten on every deploy; production boots refuse to start when the committed IDL hash ≠ `EXPECTED_IDL_HASH`. Do not skip — running prod against a drifted IDL silently corrupts every Borsh decode.
     ```bash
     cd /Users/alex/projects/turf-monster
     anchor idl fetch <PROGRAM_ID> --provider.cluster <NETWORK> > config/turf_vault.idl.json
     bin/rails solana:idl_hash                 # prints SHA256 + the heroku command
     # Run that heroku config:set, commit the IDL JSON, deploy turf-monster.
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
- **No lock instruction**: Contest can go directly from Open to Settled (Locked status exists but no instruction sets it yet)
- **2-of-3 Multisig** (v0.8.0): Treasury ops require 2 distinct signers (`admin` + `cosigner`). Routine ops require any 1-of-3. Server partially signs, human cosigns via Phantom.
- **Dual mint**: USDC + USDT supported from day one, separate vault token accounts
- **Seeds system** (v0.5.0, updated v0.7.0): Both `enter_contest` and `enter_contest_direct` award 65 seeds to the user's `UserAccount` PDA. Seeds are on-chain only — Rails reads them via `sync_balance` and derives levels in the UI (`level = seeds / 100 + 1`).
- **Manual settlement**: No on-chain scoring — Rails computes results, admin submits final rankings
- **force_close_vault**: Migration instruction that reads signers from raw bytes (avoids deserialization of old schema). Requires 2-of-3 cosign.
- **update_signers** (v0.8.0): Rotate signers or change threshold. Requires 2-of-3 cosign.

## Code Style

- Keep instructions in separate files under `instructions/`
- Thin wrappers in `lib.rs`, logic in instruction handlers
- All state in `state.rs`, all errors in `errors.rs`
- Use `msg!()` for on-chain logging at key events
- Prefer `require!()` macro over manual `if/return Err`
