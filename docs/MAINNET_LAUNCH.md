# Mainnet First Deploy — Runbook

> **HISTORICAL FIRST-DEPLOY RUNBOOK. DO NOT USE AS LIVE DEPLOYMENT IDENTITY.**
> This was the one-shot procedure for the first deployment of `turf_vault` to
> Solana mainnet-beta. Current devnet/mainnet program IDs, signer set, IDL hash,
> and upgrade authority live in [`CURRENT_DEPLOYMENT.md`](CURRENT_DEPLOYMENT.md).
> Subsequent upgrades go through `scripts/squad-upgrade.js`.

**Scope:** broadcasts an immutable program ID + locks the upgrade authority
to a Squads 2-of-3 multisig. Everything in §§3–8 is irreversible. Read the
whole runbook before starting §3.

---

## §0. Why this is separate from devnet

Mainnet differs from devnet in three ways the code already accounts for:

1. **Hardened constants** (`programs/turf_vault/src/state.rs`, gated by
   `#[cfg(feature = "mainnet")]`):
   - `INIT_AUTHORITY = 7ZDJp7FU…` (Alex Phantom) — only this key may call
     `initialize`.
   - `EXPECTED_USDC_MINT = EPjFWdd5…` (Circle USDC) — the program rejects
     any other mint for slot 0.
   - `EXPECTED_USDT_MINT = Es9vMFr…` (Tether USDT) — rejects any other
     mint for slot 1.
2. **Cluster-gated `declare_id!`** (`programs/turf_vault/src/lib.rs`).
   Default builds bake the devnet program ID; `--features mainnet` resolves
   to the mainnet program ID that §3 below will set.
3. **Upgrade authority is Squads from day 1.** `anchor deploy` runs ONCE
   (the first time), then we transfer upgrade authority to the Squads
   vault. Every subsequent change is a Squads proposal.

---

## §1. Pre-flight checklist

Complete every box before §3. Each line should be confirmable by command.

- [ ] **Funding gate.** Alex Bot has ≥ 7.5 SOL on mainnet
  - Check: `solana balance F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ --url mainnet-beta`
  - Need: ~3.45 SOL deploy rent + ~0.1 SOL init/register fees + ~4 SOL retry
    buffer = **7.5 SOL floor**. If short, fund from operator wallet before
    proceeding.
- [ ] **Devnet is healthy.** `bin/rails solana:health` (in turf-monster) is
      4/4 green pointed at devnet — proves the Rails/Solana stack is in a
      known-good state to clone from.
- [ ] **Tests green.** `cd ~/projects/turf-vault && anchor test` passes
      against localnet. `cd ~/projects/turf-monster && bin/rails test`
      green.
- [ ] **Mainnet RPC URL** stored in 1Password at `agent.helius` →
      "Mainnet RPC URL". Verify with
      `op read "op://${MCR_OP_VAULT_AGENT:-agents-studio}/agent.helius/Mainnet RPC URL"`.
- [ ] **Squads mainnet vault** stood up (§2). Members confirmed, threshold
      verified, vault PDA derived.
- [ ] **Operator wallets ready to sign.** Alex Phantom (`7ZDJp7FU…`) loaded
      and unlocked for the `initialize` step. Mason Phantom (`CytJS23…`)
      loaded for the Squads upgrade-authority-transfer approval. Both on
      mainnet-beta in Phantom's network setting.
- [ ] **mcritchie-studio + turf-monster deploys are clean** (no in-flight
      Heroku release commands waiting), so the §7 Heroku flip is the only
      change touching prod env.

---

## §2. Squads mainnet vault — provision

If the multisig vault already exists, skip to §3. (Memory note from
2026-05-26 referenced `83BX…4D1K` — verify it on the Squads V4 UI before
trusting it.)

1. Open the Squads V4 UI at https://app.squads.so/ on mainnet-beta.
2. **Create** a new multisig with:
   - Members:
     - `alex_bot`  = `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ`
     - `alex`      = `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr`
     - `mason`     = `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR`
   - Threshold: **2 of 3**
3. From the vault page, copy the **multisig PDA** and the **vault PDA**
   (under "Vaults" → main vault). These are the two addresses we need.
4. Fund the **vault PDA** with ≥ 0.05 SOL so it can pay fees on its own
   proposal executions.
5. Record both PDAs into `scripts/squad.json` — at its **top level**
   (`multisigPda`, `vaultPda`). That file has no `mainnet` block; its top level
   IS the mainnet config, and is what `squad-upgrade.js` and
   `initialize-mainnet.js` both read.
   Commit + push that update before §3.

---

## §3. Generate the mainnet program keypair

This pubkey is the immutable program ID — once §6 runs, it cannot be
changed. Generate it once, commit the pubkey (NOT the secret) into source,
back up the secret to 1Password, never paste the secret anywhere else.

```bash
cd ~/projects/turf-vault

# Fresh keypair — note --no-bip39-passphrase keeps it as a plain 64-byte
# secret so it's the same shape as every other Anchor program keypair.
solana-keygen new --no-bip39-passphrase \
  -o target/deploy/turf_vault-mainnet-keypair.json

# Print the pubkey — this is the future PROGRAM_ID.
solana-keygen pubkey target/deploy/turf_vault-mainnet-keypair.json
```

> **Back up the secret NOW.** Upload
> `target/deploy/turf_vault-mainnet-keypair.json` to 1Password as a new
> item `agent.turf-vault.mainnet-program-keypair`. If you skip this and
> lose the file, you'll have to re-do §3–§5 with a new keypair.

Replace the three placeholder `11111111111111111111111111111111` occurrences
with the printed pubkey:

1. `programs/turf_vault/src/lib.rs` — the `#[cfg(feature = "mainnet")] declare_id!(…)` line.
2. `Anchor.toml` — `[programs.mainnet]` entry.
3. `scripts/squad.json` — the **top-level** `programId` field.

Verify all three match:

```bash
grep '11111111111111111111111111111111' \
  programs/turf_vault/src/lib.rs Anchor.toml scripts/squad.json
# (should print nothing)
```

Commit (this is a meaningful, auditable change — squash with the next
mainnet-related commits or PR them in one push). Push.

---

## §4. Build the mainnet binary

```bash
cd ~/projects/turf-vault
rm -f target/deploy/turf_vault.so
anchor build -- --features mainnet
ls -la target/deploy/turf_vault.so
```

Sanity check:

- Size should be within ±5% of the devnet binary (~495KB → ~470–520KB).
- `solana-keygen pubkey target/deploy/turf_vault-mainnet-keypair.json`
  must equal the `declare_id!` value baked into the binary. Anchor's
  `idl init` step in §6 would catch a mismatch, but check now.

Compute + record the IDL hash that Heroku will pin in §7:

```bash
# Re-uses the turf-monster Rails task — assumes the IDL JSON has been
# emitted by `anchor build` into target/idl/turf_vault.json.
cp target/idl/turf_vault.json ~/projects/turf-monster/config/turf_vault.idl.json
cd ~/projects/turf-monster
bin/rails solana:idl_hash
```

Note the printed SHA256 — you'll set `EXPECTED_IDL_HASH` to this value in §7.

---

## §5. Deploy the program

```bash
cd ~/projects/turf-vault

# 1. Point solana CLI at Alex Bot's mainnet keypair (the deployer).
solana config set --url mainnet-beta \
  --keypair /path/to/alex-bot-mainnet-keypair.json   # ← from 1Password
solana balance     # must be ≥ 7.5 SOL

# 2. Broadcast.
anchor deploy --provider.cluster mainnet \
  --provider.wallet /path/to/alex-bot-mainnet-keypair.json
# Solana CLI prints: "Program Id: <PROGRAM_ID>"
# This MUST match the pubkey from §3. If not, abort — something's wrong.
```

If `anchor deploy` fails partway (network blip, retried txs), it'll print a
**buffer account**. Recovery commands are in `RUNBOOK.md` §Deploy Failures.

---

## §6. Lock down upgrade authority

The program ships with Alex Bot as upgrade authority. We need to transfer
that to the Squads vault so subsequent upgrades require 2-of-3.

```bash
solana program set-upgrade-authority <PROGRAM_ID_FROM_§3> \
  --new-upgrade-authority <SQUADS_VAULT_PDA_FROM_§2> \
  --skip-new-upgrade-authority-signer-check
```

> The `--skip-new-upgrade-authority-signer-check` is necessary because the
> new authority is a PDA, not a keypair — it can't sign the transfer itself.

Verify:

```bash
solana program show <PROGRAM_ID_FROM_§3>
# Upgrade Authority: <should match SQUADS_VAULT_PDA_FROM_§2>
```

---

## §7. Initialize + register currencies

```bash
# Switch CLI to Alex's Phantom key (INIT_AUTHORITY).
# Phantom-exported key → 1Password → temp file → solana config.
solana config set --keypair /path/to/alex-phantom-mainnet-keypair.json
solana balance     # must have ≥ 0.05 SOL to pay rent for VaultState + 2 op_rev ATAs

# Initialize the vault. This binds:
#   - admin → INIT_AUTHORITY (Alex Phantom)        (validated server-side by program)
#   - payout_mint → EPjFWdd5… (Circle USDC)        (validated)
#   - currency slot 0 → EPjFWdd5… USDC             (validated)
#   - currency slot 1 → Es9vMFr… USDT              (validated)
#   - treasury_authority → SQUADS_VAULT_PDA        (passed in as arg)
#   - signers → [alex_bot, alex, mason], threshold 2

cd ~/projects/turf-vault
node scripts/initialize-mainnet.js   # reads scripts/squad.json's top-level config
```

`scripts/initialize-mainnet.js` is the supported path; it is written, and this
step no longer has a hand-rolled alternative. Its first line of output names
the config shape and cluster it resolved. It refuses placeholder addresses, and
refuses a config that does not declare `mainnet-beta` — so a devnet config
cannot reach the mainnet mints this script hardcodes.

Verify:

```bash
# turf-monster's vault-status rake will read VaultState + show counters.
heroku run 'bin/rails runner "puts Solana::Vault.new.read_vault_state.inspect"' \
  -a turf-monster-mainnet
```

---

## §8. Spin up the mainnet Heroku app

> Production app = `turf-monster-mainnet` (separate from the devnet
> production app `turf-monster`).

```bash
# 1. Create the app if it doesn't exist.
heroku create turf-monster-mainnet \
  --buildpack heroku/ruby \
  --region us \
  --team mcritchie

# 2. Provision Postgres + Redis.
heroku addons:create heroku-postgresql:essential-0 -a turf-monster-mainnet
heroku addons:create heroku-redis:mini             -a turf-monster-mainnet

# 3. Set env vars (NEW values — do NOT copy from devnet prod).
MAINNET_URL=$(op read "op://${MCR_OP_VAULT_AGENT:-agents-studio}/agent.helius/Mainnet RPC URL")
heroku config:set \
  RAILS_MASTER_KEY=$(cat ~/projects/turf-monster/config/master.key) \
  SECRET_KEY_BASE=$(bin/rails secret) \
  RAILS_SERVE_STATIC_FILES=enabled \
  SOLANA_NETWORK=mainnet-beta \
  SOLANA_RPC_URL="$MAINNET_URL" \
  SOLANA_PROGRAM_ID=<PROGRAM_ID_FROM_§3> \
  EXPECTED_IDL_HASH=<HASH_FROM_§4> \
  BYPASS_IDL_CHECK=true \
  STRIPE_SECRET_KEY=<MAINNET-LIVE-KEY-FROM-STRIPE-DASHBOARD> \
  STRIPE_WEBHOOK_SECRET=<MAINNET-LIVE-WEBHOOK-SECRET> \
  GOOGLE_CLIENT_ID=<NEW-OR-RE-USED> \
  GOOGLE_CLIENT_SECRET=<MATCHING> \
  RESEND_API_KEY=$(op read "op://${MCR_OP_VAULT_AGENT:-agents-studio}/agent.resend/api key") \
  MAILER_FROM=alex@turfmonster.media \
  MANAGED_WALLET_ENCRYPTION_KEY=$(bin/rails secret) \
  AWS_ACCESS_KEY_ID=<from-1pass> \
  AWS_SECRET_ACCESS_KEY=<from-1pass> \
  SENTRY_DSN=<NEW-mainnet-DSN-from-Sentry> \
  -a turf-monster-mainnet

# 4. Push code.
git remote add heroku-mainnet https://git.heroku.com/turf-monster-mainnet.git
git push heroku-mainnet main

# 5. Pre-flight (boot will skip IDL check because BYPASS is on).
heroku run 'bin/rails solana:health' -a turf-monster-mainnet
# Expect: 4/4 green. If anything fails — fix it before §9.

# 6. Drop the bypass.
heroku config:unset BYPASS_IDL_CHECK -a turf-monster-mainnet
heroku run 'bin/rails solana:health' -a turf-monster-mainnet
# Re-expect: 4/4 green. The IDL hash check is now load-bearing.
```

---

## §9. Smoke test

```bash
# Browser-side: visit https://turf-monster-mainnet.herokuapp.com
# (or whatever custom domain is configured) and walk:
#   - /         → contests landing
#   - /contract → transparency page renders with the new program ID
#   - /proof-of-reserves → reads on-chain vault state
#   - /faucet   → must be disabled or no-op on mainnet (it's devnet-only)
#   - Sign in via Phantom on mainnet-beta — flow lands on /
#   - /tokens/buy → Stripe checkout in TEST mode first
#     (verify no real charges yet; ENABLE_TEST_SCAFFOLDING=false on prod)
```

If anything is wrong, the recovery flow is:
- Roll back Heroku to v(N-1): `heroku rollback -a turf-monster-mainnet`
- The on-chain program is immutable for now — no rollback there, but
  no on-chain state was written by smoke testing if you stuck to
  read-only routes.

---

## §10. Cutover

This is the moment you flip `app.turfmonster.media` to point at the
mainnet app instead of the devnet one. Up to this point the
mainnet app has no traffic and the devnet app is unchanged.

1. Update DNS / Heroku custom domain attachment.
2. Announce externally.
3. Watch Sentry + Heroku logs for the next ~hour.
4. Update `docs/CURRENT_DEPLOYMENT.md` and the McRitchie Studio agent docs with
   the live date, program ID, vault PDA, and Heroku app.

---

## Definition of done

- [ ] `solana program show <PROGRAM_ID>` shows upgrade authority =
      Squads vault PDA.
- [ ] `bin/rails solana:health` on `turf-monster-mainnet` returns 4/4
      green with `BYPASS_IDL_CHECK` unset.
- [ ] `Solana::Vault.new.read_vault_state` returns sensible values
      (admin, payout_mint, currency slots 0+1 populated, treasury_authority
      matches Squads vault).
- [ ] Smoke test in §9 passes.
- [ ] Memory entry written.
- [ ] Task #12 marked completed.

---

## References

- `RUNBOOK.md` — devnet-focused; includes Build/Deploy/Test failure
  recovery commands that apply to mainnet too.
- `CURRENT_DEPLOYMENT.md` — current authority details and Squads upgrade rule
  for subsequent releases.
- `docs/v0.16-spec.md` — full v0.16 surface.
- turf-monster `docs/SECURITY_REVIEW.md` — current app/on-chain security review
  checklist to run before any mainnet-facing upgrade.
