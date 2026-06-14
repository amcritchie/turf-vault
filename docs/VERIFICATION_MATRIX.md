# TurfVault Verification Matrix

This is the current proof checklist for the active self-custody instruction
surface. It is not a replacement for source review, and it is not deployment
identity. Live program IDs, signer set, IDL hash, and upgrade authority live in
[`CURRENT_DEPLOYMENT.md`](CURRENT_DEPLOYMENT.md).

The existing TypeScript suite still includes retired v0.15 deposit/withdraw
cases. Treat it as partial evidence until the suite is realigned to this matrix.

## Baseline Commands

```bash
anchor build
anchor test
```

If `anchor test` cannot inherit Node/Yarn, use the direct test path from
[`../RUNBOOK.md`](../RUNBOOK.md). After any source change, regenerate the IDL and
re-pin Turf Monster from the freshly built file, not from `anchor idl fetch`.

## Instruction Matrix

| Area | Instruction | Required proof |
|------|-------------|----------------|
| Vault setup | `initialize` | Creates singleton `VaultState`; pins payout mint, treasury authority, signers, threshold, USDC slot 0, USDT slot 1; mainnet build rejects non-`INIT_AUTHORITY`. |
| Governance | `update_signers` | Requires two distinct current signers; rejects duplicates, zero/default slots, and rotations that drop either authorizing signer. |
| Currency registry | `register_currency` | Requires 2-of-3; rejects duplicate mint and full registry; initializes stable `op_rev` ATA for the new slot. |
| Currency registry | `deactivate_currency` | Requires 2-of-3; flips `active=false`; preserves slot and historical tallies. |
| Pause control | `pause` | Requires 2-of-3; records reason; blocks `enter_contest` and `enter_contest_with_token` only. |
| Pause control | `unpause` | Requires 2-of-3; clears pause; paid and token entries work again. |
| User account | `create_user_account` | Permissionless payer can create a wallet account; username charset, length, and reserved-prefix checks hold. |
| User account | `set_username` | Requires owner signature; rejects non-owner, invalid charset, short names, and reserved prefixes. |
| User account | `admin_create_user_account` | Requires payer plus 1-of-3 vault signer; waives only reserved-prefix branch; still enforces charset and length. |
| User account | `admin_set_username` | Requires owner signature plus 1-of-3 vault signer; waives only reserved-prefix branch; rejects non-owner and non-signer admin. |
| Season | `create_season` | Requires 1-of-3; creates immutable entry seed schedule and quest seed schedule; rejects duplicate season ID. |
| Contest | `create_contest` | Requires 1-of-3 payer plus creator; funds prize-pool ATA; validates payout tiers sum to prize pool; stores per-currency fees and lock timestamp. |
| Contest | `set_contest_lock_time` | Requires 1-of-3 before lock; rejects invalid timestamp/order and post-finality changes without required cosign path. |
| Contest | `set_contest_conclusion_time` | Requires 1-of-3 for first set; rejects invalid timestamp/order and post-finality changes without required cosign path. |
| Entry | `enter_contest` | Requires user signature plus 1-of-3 payer; validates active currency slot, user ATA funds, max entries, lock/conclusion gate, and season schedule seed award. |
| Entry | `enter_contest_with_token` | Requires user signature plus 1-of-3 payer; consumes matching `EntryTokenAccount`; awards seeds; charges no currency and cannot be reused. |
| Free entry | `mint_entry_token` | Requires 1-of-3; PDA is keyed by `sha256(source_ref)`; remint of the same source reference fails. |
| Seeds | `grant_seeds` | Requires 1-of-3; applies bounded quest/referral seed amount; idempotent per `(wallet, kind, invitee)` guard PDA. |
| Settlement | `settle_contest` | Requires 2-of-3; requires contest locked/concluded; validates user/entry PDAs and winner ATA owner/mint; pays only from prize pool; rejects duplicate settlement pairs and over-cap payouts. |
| Cancellation | `cancel_contest` | Requires 2-of-3; refunds live prize-pool balance to creator ATA; status moves to Cancelled; operator revenue remains separate. |
| Closeout | `close_contest` | Requires 1-of-3; only settled/cancelled contests; dust-sweeps prize pool to USDC `op_rev`; closes contest PDAs. |
| Treasury | `sweep_operator_revenue` | Requires 2-of-3; drains selected currency `op_rev` to treasury ATA; enforces treasury owner equals `vault_state.treasury_authority`. |

## Cross-Repo Proof

| Consumer | Proof |
|----------|-------|
| Turf Monster IDL | `config/turf_vault.idl.json` / `config/turf_vault.mainnet.idl.json` hashes match the intended deployment target and configured `EXPECTED_IDL_HASH`. |
| Turf Monster Rails flows | Magic-link/managed-wallet and Phantom paths build transactions against current instruction names and account lists. |
| Public contract page | `/contract` instruction list, byte/caller metadata, and version labels match the newly pinned IDL. |
| Operator docs | `turf-monster/docs/SOLANA.md`, `turf-vault/README.md`, `RUNBOOK.md`, and `CURRENT_DEPLOYMENT.md` agree on program IDs, signer set, and upgrade rule. |

## Known Gaps

- The TypeScript suite should be rewritten around this 22-instruction matrix and
  should delete retired `deposit`, `withdraw`, and daily-withdraw-cap cases.
- Devnet and mainnet verification are distinct: devnet may run v0.25 while
  mainnet remains v0.24 until the next upgrade window.
- Any signer or upgrade-authority change must update `CURRENT_DEPLOYMENT.md` in
  the same change that updates deployment configuration.
