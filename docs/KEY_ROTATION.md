# TurfVault — Key-Rotation Redeploy Plan (Alex Bot key compromise)

> **HISTORICAL SUPERSEDED PLAN. DO NOT EXECUTE AS CURRENT PROCEDURE.**
> The retired Alex Bot key `F6f8...KzhZ` has zero devnet authority in the
> current deployment record. Current live signer facts live in
> [`CURRENT_DEPLOYMENT.md`](CURRENT_DEPLOYMENT.md). Current source includes
> `update_signers`, so future signer rotation should be planned from current
> source, `CURRENT_DEPLOYMENT.md`, and the Squads state rather than replaying
> this redeploy plan.
>
> **STATUS: DRAFT / PLAN ONLY. DO NOT EXECUTE.**
> This document is the redeploy runbook for rotating a leaked **Alex Bot**
> signer key off TurfVault. Execution is **gated** on:
> 1. an adversarial mini-review of v0.20 (`update_signers` + this plan), and
> 2. an explicit operator GO.
>
> Nothing in here has been run. The agent op token is **read-only** — every
> step that writes a key, signs, or broadcasts is an **operator action**.

---

## §0. Why a full redeploy (and why only once)

The leaked key is **Alex Bot** (`F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ`),
a 1-of-3 vault signer **and** a member of the Squads upgrade-authority
multisig.

Two separate things contain this key:

| Where | Mutable in place? | How |
|-------|-------------------|-----|
| Squads multisig **membership** (upgrade authority) | **YES** | Squads config tx (2-of-3) — members ARE mutable. |
| The deployed program's **VaultState.signers** | **NO** (on the *currently deployed* v0.19 program) | v0.16 removed `update_signers`, so the on-chain signer set is immutable. |

Because the *deployed* program can't rotate its own `VaultState.signers`, the
only way to evict the leaked key from the **vault signer set** is to deploy a
program that HAS `update_signers` and re-init its VaultState with the new key.
That program is **v0.20**. After this rotation, any future signer compromise is
a single 2-of-3 `update_signers` transaction — **never a redeploy again.**

> We could *technically* leave the existing v0.19 program deployed, transfer a
> v0.20 binary onto it via the Squads upgrade, and... still be stuck, because
> v0.19's VaultState is already initialized with the leaked key and v0.19 has
> no `update_signers`. Upgrading the *code* doesn't rotate *data*. So we go to a
> **fresh program ID** with a clean init. This also gives us a clean break from
> any state the leaked key may have touched, and lets us close the old program
> to reclaim rent.

### Threat-model note (do this FIRST, before anything below)

A leaked Alex Bot key is a **1-of-3** signer. On its own it can run only the
**1-of-3 routine ops** (`create_contest`, `set_contest_lock_time`,
`close_contest`, `mint_entry_token`, facilitate entries). It **cannot** settle,
cancel, sweep, register/deactivate currencies, pause/unpause, or
`update_signers` — those are 2-of-3. So the blast radius is limited UNLESS the
attacker also holds a second signer. **But** Alex Bot is also a Squads member;
two compromised Squads members could push a malicious program upgrade.

Immediate containment (operator, before the redeploy):
1. **`pause` the vault** (2-of-3: Alex + Mason, NOT the compromised bot) to
   block `enter_contest{,_with_token}` while you rotate. Pause does NOT block
   the 1-of-3 ops the leaked key could still call, so also:
2. **Rotate the SOLANA_ADMIN_KEY env off the leaked key** wherever it lives
   (Heroku devnet prod, dev `.env`, 1Password `agent.solana`) so the server
   stops *using* it — but the on-chain set still trusts it until §5.
3. Treat the Squads membership rotation (§7) and old-program close (§8) as
   part of the same incident, not optional cleanup.

---

## Identities

| Role | OLD pubkey | NEW |
|------|-----------|-----|
| Alex Bot (server, **LEAKED**) | `F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ` | generated in §1 |
| Alex (human) | `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr` | unchanged |
| Mason | `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR` | unchanged |
| INIT_AUTHORITY (mainnet const) | `7ZDJp7FU…` (Alex Phantom) | unchanged — re-init runs from Phantom |

> **INIT_AUTHORITY is Alex's Phantom key, NOT Alex Bot.** It is a compile-time
> constant in `state.rs` and is unaffected by the leak. The §5 re-init runs
> from Phantom, so the leaked key never touches the new program.

Mainnet program / Squads (from `scripts/squad.json` `mainnet` block):

| | value |
|--|--|
| OLD program ID | `mnzowM2F9dppGVFGrcTAh5351mMqYunX3b2MvdvgS2S` |
| OLD Squads vault PDA | `83BXrVFBNxonkiTsWm7ZPetLqKEvEGcGhALjVbLm4D1K` |
| OLD Squads multisig PDA | `9dCLMZctmoo3gZj9BGPWD9ueqBx2zpMhpVGU53sRknJr` |
| NEW program ID | generated in §3 |
| Squads vault for new program | **recommend a NEW Squads** (see §4) |

> This plan is written for **mainnet** (`--features mainnet`), since that's the
> deployment whose signer set is load-bearing for real funds. If you're
> rehearsing on **devnet** first (strongly recommended), substitute the devnet
> program ID (`EQGFJAcA…`), devnet Squads (`7nRuVw3…` / `BW13kgfi…`), and a
> devnet Alex Bot, and use the default (non-mainnet) build.

---

## §1. Generate a NEW Alex Bot keypair  *(operator writes; agent op token is read-only)*

```bash
cd ~/projects/turf-vault

# Fresh server keypair. Plain 64-byte secret (same shape as the old one).
solana-keygen new --no-bip39-passphrase \
  -o /tmp/alex-bot-new-keypair.json

# Record the NEW pubkey — used in §3 (deployer), §5 (vault signer), §6 (env).
solana-keygen pubkey /tmp/alex-bot-new-keypair.json
```

> **Back up the secret NOW, then shred the temp file.** Operator uploads
> `/tmp/alex-bot-new-keypair.json` to 1Password as a **new** document item —
> propose `agent.solana` (overwrite the leaked `private key` field) plus a
> dated backup `agent.solana.rotated-2026-06`. The agent **cannot** do this
> write (read-only token); the operator runs it. After backup:
> `shred -u /tmp/alex-bot-new-keypair.json` once §1–§6 are done.

> **Do NOT reuse the leaked secret anywhere.** Never paste either secret inline;
> use the `pbpaste`-from-1Password pattern (`bin/setup-1pass-token`).

---

## §2. Fund the new Alex Bot  *(operator signs)*

Transfer the old Alex Bot's balance to the new one, leaving dust. The leaked
key can still *sign a send* (it's not frozen), so do this promptly — but it's
also the LAST useful thing the old key does.

```bash
# Point CLI at the OLD (leaked) Alex Bot keypair (from 1Password → temp file).
solana config set --url mainnet-beta --keypair /tmp/alex-bot-OLD-keypair.json
solana balance F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ
#   → ~4.6 SOL expected

# Transfer ~4.55 SOL to the NEW bot, leaving ~0.05 for fees/dust.
solana transfer <NEW_ALEX_BOT_PUBKEY> 4.55 \
  --allow-unfunded-recipient \
  --keypair /tmp/alex-bot-OLD-keypair.json
```

> Leave the old key with dust, not zero — a zeroed account can complicate the
> later forensic trail and any final Squads-removal fee. ~0.05 SOL is plenty.
> Confirm `solana balance <NEW_ALEX_BOT_PUBKEY>` reflects the transfer before §5.

---

## §3. Deploy v0.20 to a NEW program ID  *(operator signs; mirrors MAINNET_LAUNCH §3–§5)*

> v0.20 is already **built** (devnet + `--features mainnet`) and pending the
> adversarial mini-review. Do NOT re-deploy v0.19. The fresh program ID gives a
> clean VaultState with the new signer set baked in at init (§5).

```bash
cd ~/projects/turf-vault

# 3a. New program keypair (immutable program ID).
solana-keygen new --no-bip39-passphrase \
  -o target/deploy/turf_vault-mainnet-keypair.json
solana-keygen pubkey target/deploy/turf_vault-mainnet-keypair.json   # = NEW PROGRAM ID
```

> **Back up `target/deploy/turf_vault-mainnet-keypair.json` to 1Password**
> (`agent.turf-vault.mainnet-program-keypair`, overwriting the old) before
> deploying. Operator action.

Replace the 3 program-ID references with the NEW program ID (same edit points
as MAINNET_LAUNCH §3):
1. `programs/turf_vault/src/lib.rs` — `#[cfg(feature = "mainnet")] declare_id!(…)`
2. `Anchor.toml` — `[programs.mainnet]`
3. `scripts/squad.json` — `mainnet.programId`

```bash
# 3b. Rebuild mainnet binary with the new declare_id! baked in.
rm -f target/deploy/turf_vault.so
anchor build -- --features mainnet
# Sanity: the keypair pubkey MUST equal the declare_id! in the binary.
solana-keygen pubkey target/deploy/turf_vault-mainnet-keypair.json

# 3c. Deploy from the NEW Alex Bot (deployer = initial upgrade authority).
solana config set --url mainnet-beta --keypair /tmp/alex-bot-new-keypair.json
solana balance   # must be ≥ ~3.5 SOL for ProgramData rent + fees
anchor deploy --provider.cluster mainnet \
  --provider.wallet /tmp/alex-bot-new-keypair.json
# "Program Id: <NEW PROGRAM ID>" — MUST match 3a. If not, ABORT.
```

> **~3.5 SOL is consumed as permanent ProgramData rent** on the new program
> (recovered from the old program in §8 minus a small net cost — see §9). The
> new Alex Bot needs that balance available at deploy time; §2 funds it.

---

## §4. Transfer upgrade authority to a Squads vault  *(operator)*

**Recommendation: stand up a NEW Squads multisig for the new program** rather
than reusing `9dCLM…`. Rationale: the existing Squads *membership still
contains the OLD (leaked) Alex Bot* `F6f8…`. Reusing it means the new program's
upgrade authority is, transiently, a multisig that includes a compromised
member — until §7 rotates it out. A clean new Squads (members = NEW alex_bot,
alex, mason; threshold 2) avoids that transient and avoids ever pointing the
new program at a tainted authority.

> If the operator prefers to reuse `9dCLM…` to save the Squads-creation cost:
> it's *acceptable* **only if §7 (rotate old bot OUT of membership) runs
> BEFORE** any real funds flow, because Squads members ARE mutable. But the
> recommended path is new-Squads-now, and rotate the OLD Squads' membership
> separately for the §8 close vote.

```bash
# 4a. (Recommended) Create the new Squads 2-of-3 via https://app.squads.so
#     (mainnet). Members: NEW alex_bot, alex 7ZDJ, mason Cyt. Threshold 2.
#     Record the new multisigPda + vaultPda into scripts/squad.json mainnet block.

# 4b. Point the new program's upgrade authority at the new Squads vault PDA.
solana program set-upgrade-authority <NEW_PROGRAM_ID> \
  --new-upgrade-authority <NEW_SQUADS_VAULT_PDA> \
  --skip-new-upgrade-authority-signer-check \
  --keypair /tmp/alex-bot-new-keypair.json

# 4c. Verify.
solana program show <NEW_PROGRAM_ID>
#   Upgrade Authority: <should match NEW_SQUADS_VAULT_PDA>
```

---

## §5. Re-init the vault with the new signer set  *(operator signs from Phantom)*

`initialize` is gated to `INIT_AUTHORITY` = Alex Phantom (`7ZDJ…`), a
compile-time constant — **unaffected by the leak**. So the re-init runs from
Phantom; the leaked key never touches the new program.

```bash
# Switch CLI to Alex's Phantom key (INIT_AUTHORITY). Phantom → 1Password → temp.
solana config set --url mainnet-beta --keypair /tmp/alex-phantom-keypair.json
solana balance   # ≥ ~0.05 SOL for VaultState + 2 op_rev ATAs rent

cd ~/projects/turf-vault
# Edit scripts/initialize-mainnet.js (or squad.json mainnet block) so signers =
#   [ NEW alex_bot, 7ZDJ alex, Cyt mason ], threshold 2,
#   treasury_authority = NEW_SQUADS_VAULT_PDA.
node scripts/initialize-mainnet.js
```

Verify the new VaultState carries the **new** signer set and **NOT** the leaked
key:

```bash
heroku run 'bin/rails runner "puts Solana::Vault.new.read_vault_state.inspect"' \
  -a turf-monster-mainnet
#   signers MUST be [NEW alex_bot, 7ZDJ, Cyt]; F6f8… MUST be absent; threshold 2.
```

> After §5, **`update_signers` is live on this program.** Any future signer
> compromise is now a 2-of-3 `update_signers` tx (Alex + Mason cosign, evict the
> bad key, keep continuity) — no redeploy. That is the whole point of v0.20.
>
> **Continuity rule for every future `update_signers` (2-of-3, R7):** the new
> set MUST keep BOTH human cosigners — Alex (`7ZDJ…59Tcr`) and Mason
> (`Cyt…qWjrR`) — and MUST NEVER rotate down to a single operator-controlled
> key. Threshold is pinned at 2-of-3, so a multisig needs TWO surviving
> cosignable keys; the v0.20 guard rejects (`SignerContinuityRequired` 6017)
> any set that drops either authorizing cosigner. **After the rotation, the
> VaultState read-back MUST confirm TWO controlled keys survive** (both Alex
> and Mason present), not just one — a `[survivor, junk, junk]` set would brick
> all governance even though it superficially "kept a known-good key."

> **WHO BUILDS AND COLLECTS THE TWO SIGNATURES — recorded 2026-09-04, because
> nothing here said.** The rule above says a rotation *is* a 2-of-3 tx; it never
> said what composes it or gathers the signatures.
>
> **There was one built tool, and it is gone.** McRitchie Studio's admin signing
> console composed the instruction server-side and let each signer approve in
> their own Phantom, anchored on a durable nonce so a half-signed transaction did
> not expire between signers. It was **deleted on 2026-09-04**
> (`/tasks/retire-signing-console`, merged to `accepted`; it reaches production
> with the next release): Turf Monster is the hub for all Solana/web3 logic, so
> the hub keeps none. Nothing in this repo ever referenced it, so nothing here
> broke — but it was the only tool that did this, and a reader who finds it in an
> older audit should know it is not the path.
>
> **Squads does not cover this.** A Squads V4 vault PDA holds the program's
> **upgrade** authority — on **mainnet**, which this plan is written for, that is
> `Bk9sS7iiSRL18vuo2KVzkeGw7EekKqxMCjrdoyGGdJm` (`scripts/squad.json`, the live
> top-level block). `BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC` is the
> **devnet** vault PDA (`CURRENT_DEPLOYMENT.md` § Devnet, and squad.json's
> `_devnet_reference`); the `Identities` section above already lists it as the
> devnet substitution. The vault's **signer set** is a different authority
> entirely, changed by a different transaction: `update_signers` against
> `VaultState`, never a Squads proposal.
>
> **But it is the same three keys — verify that before you rely on it.** The
> Squads membership is `8K81…` (Alex Bot), `7ZDJ…` (Alex), `Cyt…` (Mason)
> (`scripts/squad.json` `members`). The mainnet **VaultState** signer set is
> **not recorded in this repo**: `CURRENT_DEPLOYMENT.md`'s only signer rows sit
> under `## Devnet`, and its `## Mainnet` table has none. Read it on-chain with
> the §5 read-back above before composing the rotation. Two authorities, two
> transactions, **one overlapping membership**: a compromised key sits in BOTH,
> which is why §0 calls Alex Bot "a 1-of-3 vault signer **and** a member of the
> Squads upgrade-authority multisig," and why **§7 exists**. Rotating the vault
> signer set does **not** evict that key from Squads. A real compromise needs
> both moves.
>
> **So a rotation today needs a script that does not exist yet.** The only
> chain-operation scripts in `scripts/` are `initialize-mainnet.js` and
> `squad-upgrade.js` — the rest of the directory is `squad.json` (the config both
> read, and the one §4/§5 have you edit) and `check-doc-op-refs.js` (the docs
> guard CI runs). Neither builds `update_signers`.
>
> **WHO MAY SIGN IT.** (Every Rust file cited below lives under
> `programs/turf_vault/src/` — `state.rs` and `errors.rs` at its root,
> `update_signers.rs` and `initialize.rs` in its `instructions/`.) The
> transaction carries TWO `Signer` accounts, `admin` and `cosigner`
> (`update_signers.rs:48-51`), and the constraint that decides
> acceptance is `validate_multisig(&admin.key(), &cosigner.key())`
> (`update_signers.rs:57`), which is
> `s1 != s2 && is_signer(s1) && is_signer(s2)` (`state.rs:159-160`). So both
> keys must be **distinct**, and both must ALREADY be members of
> `VaultState.signers` — the set as it stands BEFORE the write. Neither slot is
> privileged: `admin` and `cosigner` are symmetric, so either eligible key may
> occupy either slot. Any other pair returns `Unauthorized` (**6000**,
> `update_signers.rs:58`), whose message — "Only the vault admin can perform
> this action" (`errors.rs:20-21`) — misdescribes the check it reports: there is
> no single admin here, only two current signers.
>
> **2-of-3 is structural, not configured.** `validate_multisig` never reads
> `VaultState.threshold` — and neither does anything else that authorizes. The
> field's only reads in the whole program are in `initialize.rs`, which
> validates it 1-3 (`:112`), stores it (`:142`) and logs it (`:177`); no
> instruction consults it afterwards. The "2" is the function's arity plus its
> distinctness test, the "3" is `signers: [Pubkey; 3]`, and the same
> `validate_multisig` gates every other 2-of-3 instruction in the program
> (`settle_contest`, `cancel_contest`, `pause`, `sweep_operator_revenue` and the
> rest). So setting `threshold` to anything else would change nothing.
> `update_signers` also does not write it: its handler's only state write is
> `vault.signers = new_signers` (`update_signers.rs:109-114`).
>
> **The delta is FIVE changes to `initialize-mainnet.js`, not one line.** The
> signing MECHANISM is one line and it is correct: add `.signers([cosigner])` to
> the `.rpc()` call. That script builds a single `anchor.Wallet` provider
> (`:105`) which signs as fee payer and as `admin` (`:126`) and passes no
> signers array, so the extra keypair supplies `cosigner`. Around that line,
> five things change:
>
> 1. **The method** — `.updateSigners(newSigners)` replaces
>    `.initialize(signers, threshold, treasuryAuth)` (`:124`).
> 2. **The accounts** — three (`admin`, `cosigner`, `vaultState`) replace the
>    nine at `:125-135`. `cosigner` has no counterpart in `initialize`; it is
>    added, not renamed.
> 3. **A second signing key** — `loadKeypair` (`:58-61`) is called once, for the
>    admin (`:95`). See the env-var note below for the shape the second should
>    take.
> 4. **Delete the existence guard** — `:119-120` aborts when `VaultState`
>    already exists, which is exactly the precondition a rotation requires. Left
>    in, it refuses every rotation it could ever be asked to perform.
> 5. **The config read is already broken** — `:67` aborts unless `cfg.mainnet`
>    exists, and `:71-83` read `programId`, `vaultPda`, `members` and
>    `threshold` off that block. `scripts/squad.json` has NO `mainnet` key — its
>    top level is `_comment`, `network`, `programId`, `multisigPda`, `vaultPda`,
>    `threshold`, `members`, `_devnet_reference` — so the script aborts at `:67`
>    today, before it ever reaches `.rpc()`.
>
> Items 4 and 5 are pre-existing script state, not work this rotation creates.
> They are listed because this note points an operator at that script as a
> template, which is what puts them in the path. Item 5 reaches wider than the
> script: the `Identities` table, §3's re-pin list, §4 step 4a and §5's edit
> step all still say `squad.json`'s "`mainnet` block" as well. Whether the
> repair belongs in the script (read the top level) or in the config (restore a
> `mainnet` block) is a separate decision with its own task — **it is NOT fixed
> here**, and it is recorded so nobody rediscovers it under pressure.
>
> Do **not** model the script on §5: §5 is a single-signer `INIT_AUTHORITY` flow
> with no partial-signature step in it. With two LOCAL keypairs there are no
> partial signatures to collect and no expiry window to race — the durable nonce
> the console needed was needed only because its signers were REMOTE, in
> separate browsers. **Budget the work into the rotation window; do not discover
> it there.**
>
> **Its cost, and how to keep it small.** Both keys leave Phantom, which is
> precisely the property the console existed to avoid; that is traded knowingly.
> But do not reach for keypair FILES by default. `squad-upgrade.js` — this repo's
> other two-signature script — loads both signing keys from base58 env vars
> (`ALEX_BOT_KEY` / `MASON_KEY`), explicitly "never argv — argv leaks in `ps`",
> and never writes them to disk. Match that — the HANDLING, not the pair.
> Those two variables name Alex Bot and Mason because a Squads UPGRADE is what
> `squad-upgrade.js` signs. For the rotation THIS runbook is about — evicting a
> compromised Alex Bot — the signing pair MUST be **Alex
> (`7ZDJp7FU…59Tcr`) and Mason (`CytJS23p…qWjrR`)**, the two human mainnet vault
> signers in the `Identities` table above, and the bot MUST NOT sign.
> Continuity (`update_signers.rs:100-107`) requires BOTH authorizing cosigners
> to survive into the new set, so a bot signature forces the bot to stay:
> dropping it then trips `SignerContinuityRequired` (**6017**,
> `errors.rs:54-55`), and satisfying the guard means keeping the leaked key.
> Either way the eviction fails. Name the env vars for the keys that actually
> sign. §5's
> `/tmp/alex-phantom-keypair.json` is a single-signer `solana` CLI convenience
> (`initialize-mainnet.js` reads `SOLANA_ADMIN_KEYPAIR`, a path), not the pattern
> for this shape of transaction. If a temp keypair file is unavoidable anyway,
> treat it as burned — 1Password to `/tmp`, shredded with the rotation, never
> reused. If browser coordination is wanted instead, **build it in turf-monster**
> (the web3 app, per `app-templates.md` in mcritchie-studio). That is **not
> filed** and this note is not a request to file it: the console served zero
> production signing requests in its lifetime, so the tool is worth building when
> a rotation is actually scheduled — not before.

---

## §6. Re-pin IDL + re-point turf-monster-mainnet env  *(operator)*

The new program's mainnet IDL hash must be pinned, and the Rails env re-pointed
to the new program + new admin key. Use the **freshly-built** IDL from
`target/idl/turf_vault.json` (the §3b build), **NOT `anchor idl fetch`** — a
fresh deploy + Squads-authority program does not get its on-chain IDL account
updated the way a single-key `anchor deploy` would, so a fetch could be stale.

```bash
cp target/idl/turf_vault.json ~/projects/turf-monster/config/turf_vault.idl.json
cd ~/projects/turf-monster
shasum -a 256 config/turf_vault.idl.json   # = NEW EXPECTED_IDL_HASH

# Set on Heroku BEFORE pushing (assets:precompile runs verify_idl!).
heroku config:set \
  SOLANA_PROGRAM_ID=<NEW_PROGRAM_ID> \
  EXPECTED_IDL_HASH=<NEW_HASH> \
  SOLANA_ADMIN_KEY=<NEW_ALEX_BOT_BASE58_SECRET> \
  -a turf-monster-mainnet
# Then commit the new IDL JSON + git push heroku-mainnet main.
```

Also confirm:
- `SOLANA_PROGRAM_ID` is the **source of truth** (the `Solana::Config`
  fallback literal is stale — never rely on it).
- **Restart Sidekiq** after the PROGRAM_ID swap and confirm a single PID
  (`Solana::Vault.ensure_program_id_live!` guard in TokenPurchaseJob).
- `SOLANA_ADMIN_KEY` now holds the **NEW** Alex Bot secret (base58), pulled
  from 1Password. Old value purged.

---

## §7. Rotate OLD Alex Bot out of the Squads membership  *(operator, 2-of-3)*

Squads members ARE mutable. The leaked key must be evicted from **every**
Squads it's still a member of — at minimum the OLD Squads (`9dCLM…`), which
still governs the OLD program (needed live for the §8 close vote).

```
Via https://app.squads.so (mainnet) on the OLD multisig 9dCLM…:
  - Propose a config tx: removeMember(F6f8… OLD Alex Bot),
    addMember(<NEW_ALEX_BOT_PUBKEY>), keep threshold 2.
  - Approve with Alex (7ZDJ) + Mason (Cyt) — the two CLEAN signers.
    Do NOT approve with the compromised bot.
  - Execute.
```

> Sequence matters: keep the old Squads usable for the §8 close vote. You can
> rotate its membership (§7) before OR after §8 as long as the **two clean
> members** (Alex + Mason) are always ≥ threshold. Recommended: rotate
> membership first (§7), then run the §8 close vote with the clean post-rotation
> set — that way the close proposal is approved by a membership that no longer
> contains the leaked key at all.

---

## §8. Close the OLD program to reclaim rent  *(operator, Squads 2-of-3)*

The OLD program (`mnzow…`) is now dead — nothing points at it. Close it to
reclaim its ~3.5 SOL ProgramData rent. The close authority is the OLD program's
upgrade authority = the OLD Squads vault, so this is a Squads proposal voted by
**Alex + Mason** (the clean members), NOT the compromised bot.

```
solana program close <OLD_PROGRAM_ID> --recipient <NEW_ALEX_BOT_OR_TREASURY>
  …wrapped as a Squads vault transaction on 9dCLM… (or the new Squads if you
  moved authority), proposed + approved by Alex + Mason + executed.
```

> `solana program close` permanently bricks the program ID and refunds
> ProgramData rent to `--recipient`. Send it to the NEW Alex Bot or the
> treasury. **Double-check the program ID** — closing is irreversible; closing
> the *new* program by mistake would force ANOTHER redeploy.
>
> If for any reason the OLD Squads can't muster 2 clean votes (e.g. authority
> was never transferred cleanly), the rent is simply stranded (~3.5 SOL) — the
> same outcome as the historically orphaned `7Hy8…`/`Dx8u…` IDs. Don't block
> the rotation on the close; it's recovery, not security.

---

## §9. Net cost accounting

| Item | SOL | Notes |
|------|----:|-------|
| New program ProgramData rent (§3) | **−3.5** | permanent on the new program |
| Reclaimed from OLD program close (§8) | **+3.5** | net ~0 if §8 succeeds |
| New VaultState + 2 op_rev ATA rent (§5) | **−0.04** | permanent (rent-exempt reserves) |
| New Squads creation (§4, if new) | **−~0.01** | one-time, recommended path |
| Tx fees across §2–§8 | **−~0.01** | negligible |
| Old Alex Bot dust left behind (§2) | **−0.05** | intentional, recoverable later |
| **Net permanent cost** | **≈ −0.04 to −0.10 SOL** | + the stranded ~3.5 SOL **only if §8 fails** |

The dominant risk to cost is **§8 failing** — if the old program can't be
closed, the real cost balloons by the stranded ~3.5 SOL ProgramData rent.
Prioritize a clean upgrade-authority + Squads state so §8 can execute.

---

## Risk register

| # | Risk | Mitigation |
|---|------|------------|
| R1 | **Leaked key acts during the window.** It's 1-of-3, so it can `create_contest` / `mint_entry_token` / `close_contest` / facilitate entries until §5. | §0 containment: `pause` first, rotate `SOLANA_ADMIN_KEY` env off it, then redeploy promptly. |
| R2 | **Two-key compromise.** If a SECOND signer is also compromised, the attacker has 2-of-3 → can settle/sweep/`update_signers`/push a Squads upgrade. | Out of scope of a single-key rotation. If suspected, freeze funds (sweep to a fresh cold treasury via the clean signers) before anything else, and treat as a full incident. |
| R3 | **Transient tainted authority** if §4 reuses the old Squads (still contains leaked bot). | Recommended path stands up a NEW Squads; if reusing, run §7 before real funds flow. |
| R4 | **Closing the wrong program** in §8. | Triple-check the program ID; close is irreversible. New program ID is in `scripts/squad.json` after §3. |
| R5 | **IDL drift** — re-pinning from a stale `anchor idl fetch` instead of the built IDL. | §6 uses `target/idl/turf_vault.json` from the §3b build; verify the hash matches `EXPECTED_IDL_HASH` before push. |
| R6 | **Sidekiq runs on the old PROGRAM_ID** after the env swap. | §6: restart Sidekiq, confirm one PID, rely on `ensure_program_id_live!` guard. |
| R7 | **Continuity guard rejects the §5 set** — N/A for re-init (init isn't `update_signers`), but relevant for FUTURE rotations: a 2-of-3 rotation must keep BOTH authorizing cosigners + no default slots, else 6017. A rotation that keeps only ONE controlled key (e.g. `[Alex, junk, junk]`) bricks all governance — no second key can ever cosign — even though a weaker "keep ≥1" guard would have passed it. | **Documented in v0.20 (continuity now requires BOTH cosigners survive).** Any future `update_signers` MUST keep both human cosigners — Alex (`7ZDJ…59Tcr`) and Mason (`Cyt…qWjrR`) — and MUST NOT rotate down to a single operator-controlled key. After the rotation, the post-rotation VaultState read-back MUST confirm TWO controlled keys survive (not just one): both Alex and Mason present in `signers`, and only the leaked key evicted. |
| R8 | **1Password write blocked** — agent op token is read-only. | Every key write (§1, §3a backups, §6 env secret) is an explicit operator action; the agent only drafts/plans. |

---

## Pre-execution gate

- [ ] **Adversarial mini-review of v0.20** (`update_signers` continuity logic +
      zero-copy `load_mut` correctness + this plan) — REQUIRED before any
      deploy.
- [ ] Operator GO.
- [ ] Rehearse the full §1–§8 on **devnet** first (substitute devnet program /
      Squads / a devnet Alex Bot).
- [ ] Hand the production rollout to **Steffon** (QA + Infra) per the standard
      mainnet rollout protocol.

> Nothing in this document is executed by the agent. Builds are done; deploys
> and on-chain writes are operator actions, gated on the review + GO above.
