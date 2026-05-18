# Runbook -- TurfVault (Anchor Program)

Troubleshooting guide for autonomous agents. Format: problem, diagnosis, fix.

## Build Failures

**Rust version mismatch**
- Diagnosis: `anchor build` fails with compiler errors or feature-gate issues. `rust-toolchain.toml` specifies Rust 1.89.0.
- Fix: `rustup install 1.89.0 && rustup default 1.89.0`. Verify: `rustc --version`. The toolchain file should auto-select but sometimes `rustup override` is needed: `cd /Users/alex/projects/turf-vault && rustup override set 1.89.0`.

**Anchor version mismatch**
- Diagnosis: `anchor build` fails with IDL generation errors or unexpected syntax. Project uses Anchor 0.32.1.
- Fix: Check version: `/Users/alex/.cargo/bin/anchor --version`. Install correct version: `cargo install --git https://github.com/coral-xyz/anchor avm --locked && avm install 0.32.1 && avm use 0.32.1`. Or directly: `cargo install anchor-cli --version 0.32.1 --locked`.

**`anchor build` out of memory or very slow**
- Diagnosis: Rust compilation is memory-intensive. Build can take several minutes on first run.
- Fix: Close memory-heavy apps. For incremental builds, `anchor build` reuses cached artifacts in `target/`. A clean build: `cargo clean && anchor build`.

**Missing Solana CLI**
- Diagnosis: `anchor build` or `anchor deploy` can't find `solana`. PATH doesn't include Solana CLI.
- Fix: Add to PATH: `export PATH="/Users/alex/.local/share/solana/install/active_release/bin:$PATH"`. Verify: `solana --version`.

## Deploy Failures

**Insufficient SOL for deployment**
- Diagnosis: `anchor deploy --provider.cluster devnet` fails with "insufficient funds". Program deploys cost ~3-5 SOL depending on binary size.
- Fix: Check balance: `solana balance --url devnet`. Fund the deploy wallet using the faucet protocol (see below). The deploy wallet is `~/.config/solana/id.json`.

**Program too large**
- Diagnosis: `Error: Deploying program failed: ... program size exceeds maximum`. Anchor programs have a ~10MB deployed limit.
- Fix: Check binary size: `ls -la target/deploy/turf_vault.so`. Reduce program size: remove unused instructions, consolidate error messages, use `msg!()` sparingly. Enable size optimization in `Cargo.toml`: `[profile.release] opt-level = "z"`.

**Program authority mismatch**
- Diagnosis: `anchor deploy` fails because the deploy key doesn't match the program's upgrade authority.
- Fix: Check authority: `solana program show 7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J --url devnet`. The upgrade authority must match `~/.config/solana/id.json`. If it doesn't, use the correct keypair: `anchor deploy --provider.cluster devnet --provider.wallet <correct_keypair.json>`.

## Test Failures

**`anchor test` can't find node/yarn**
- Diagnosis: Anchor CLI 0.32.1 spawns a Rust subprocess that doesn't inherit the full shell PATH. `node` and `yarn` are not found.
- Fix: Run tests directly without Anchor's test orchestrator:
```bash
# Start local validator in one terminal:
solana-test-validator

# Run tests in another terminal:
ANCHOR_PROVIDER_URL=http://127.0.0.1:8899 \
ANCHOR_WALLET=~/.config/solana/id.json \
yarn run ts-mocha -p ./tsconfig.json -t 1000000 tests/**/*.ts
```

**Local validator not running**
- Diagnosis: Tests fail with `Connection refused` to `127.0.0.1:8899`.
- Fix: `anchor test` starts a validator automatically, but direct test runs need a manual one. Start it: `solana-test-validator` (runs in foreground). To reset state: `solana-test-validator --reset`. The `test-ledger/` directory stores validator state.

**Test account already initialized**
- Diagnosis: `Error: Account already in use` during `initialize` instruction. The local validator has state from a previous run.
- Fix: Reset the validator: `solana-test-validator --reset`. This clears all accounts and starts fresh.

**TypeScript type errors**
- Diagnosis: `yarn run ts-mocha` fails with type errors from `@coral-xyz/anchor` or `@solana/web3.js`.
- Fix: `cd /Users/alex/projects/turf-vault && yarn install`. Check `package.json` for version compatibility. If types changed after an Anchor CLI update, regenerate the IDL: `anchor build` (updates `target/idl/turf-vault.json` and `target/types/turf-vault.ts`).

## Devnet SOL Acquisition (Faucet Protocol)

Follow in order. Move to next step only if current fails.

1. **PoW faucet** (preferred): `devnet-pow mine --target-lamports 5000000000 -ud` (5 SOL). If public RPC times out: `devnet-pow mine --target-lamports 5000000000 -ud -u <provider_rpc_url>`. Install: `cargo install devnet-pow`.
2. **QuickNode web faucet**: https://faucet.quicknode.com/solana/devnet -- paste wallet address, no account needed.
3. **Solana Foundation faucet**: https://faucet.solana.com -- select Devnet, paste address.
4. **CLI airdrop** (last resort): `solana airdrop 1 --url devnet`. Try 0.5 if 1 fails. Frequently rate-limited.
5. **Transfer from funded wallet**: `solana transfer <to_address> <amount> --url devnet --keypair <funded_wallet.json>`.

Check balance: `solana balance --url devnet` (uses default keypair) or `solana balance <address> --url devnet`.

## Vault Re-initialization (Migration)

**When to re-initialize**: After changing `VaultState` struct layout (adding/removing fields). The old account can't be deserialized with the new schema.

**Procedure** (run from the Turf Monster Rails app):
```bash
# Step 1: Close the old vault (recovers rent SOL to admin)
cd /Users/alex/projects/turf-monster
bin/rails solana:init_vault FORCE_CLOSE=true

# Step 2: Initialize new vault with updated schema
bin/rails solana:init_vault INIT=true ADMIN_BACKUP=7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr
```

**If force_close fails**: The `force_close_vault` instruction reads admin from raw bytes (bypasses Anchor deserialization). If even that fails, the old program may need to be redeployed with a compatible `force_close` handler.

**After re-init**: Verify vault state: `bin/rails runner "puts Solana::Vault.fetch_vault_state.inspect"`. Check admin and admin_backup are set correctly.

## Transaction Failures

**Account already initialized (PDA collision)**
- Diagnosis: `Error: custom program error: 0x0` (Anchor's "account already in use"). Trying to create a PDA that already exists.
- Fix: For UserAccount: user already has one. For Contest: contest_id (SHA256 of slug) collides -- use a unique slug. For ContestEntry: same entry_num for the same user+contest. Check PDA derivation seeds match expectations.

**PDA derivation mismatch**
- Diagnosis: Client-side PDA doesn't match what the program expects. Transaction fails with "A seeds constraint was violated".
- Fix: Verify seeds exactly match the program. Seeds reference (from `state.rs`):
  - VaultState: `[b"vault"]`
  - UserAccount: `[b"user", wallet_pubkey_bytes]`
  - Contest: `[b"contest", contest_id_32_bytes]`
  - ContestEntry: `[b"entry", contest_id_32_bytes, wallet_pubkey_bytes, entry_num_le_bytes]`

**Unauthorized (error 6000)**
- Diagnosis: Non-admin tried an admin action. `VaultState.is_admin()` checks both `admin` and `admin_backup`.
- Fix: Verify the signing key is one of the two admin keys. Primary: `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ`. Backup: `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`. Check `SOLANA_ADMIN_KEY` env var in the Rails app.

**Settlement overflow (error 6008)**
- Diagnosis: Total payouts in settlement exceed `prize_pool + bonus`. The `settle_contest` instruction validates this.
- Fix: Check the Rails grading logic. `Contest#grade!` must ensure `entries.sum(:payout_cents)` <= total pool. Convert cents to u64: `amount_cents * 10_000`.

## Verifying Deployment

**Check program exists**
```bash
solana program show 7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J --url devnet
```
Shows: authority, data length, balance, deploy slot.

**Check vault state**
```bash
# From Turf Monster Rails console:
bin/rails runner "puts Solana::Vault.fetch_vault_state.inspect"
```

**Check IDL is published**
```bash
anchor idl fetch 7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J --provider.cluster devnet
```
IDL account: `DCP2XRu8ZwzsCpXBgu5xa4vTYdYQhKUZRU49iJuFv8Lf`.

**Compare deployed vs local binary**
```bash
anchor build
anchor verify 7Hy8GmJWPMdt6bx3VG4BLFnpNX9TBwkPt87W6bkHgr2J --provider.cluster devnet
```
