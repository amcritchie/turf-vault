# turf-vault — Security Audit (Adversarial / Lazarus-persona)

**Date:** 2026-05-31
**Master report:** [`turf-monster/docs/SECURITY_AUDIT_2026_05_31.md`](https://github.com/amcritchie/turf-monster/blob/main/docs/SECURITY_AUDIT_2026_05_31.md) — full cross-repo report, methodology, refuted appendix, and un-covered surface live there.
**This companion:** the on-chain (turf-vault) findings only, for the contract remediation release.

> **Historical audit artifact.** Do not treat this file as the current deploy
> gate without checking source. Several findings below were remediated after
> 2026-05-31, including canonical winner ATA binding in settlement,
> lock/conclusion-time settlement gates, and `mint_entry_token` idempotency via
> `source_ref_hash`. Current deployment identity lives in
> [`CURRENT_DEPLOYMENT.md`](CURRENT_DEPLOYMENT.md); current implementation truth
> lives in `programs/turf_vault/src/`.

---

> **Original deploy gate, now superseded:** the program-side highs below
> required a turf-vault upgrade via the Squads 2-of-3 vault before the
> corresponding Rails changes went live. Keep this as historical rationale; use
> current source/tests/deployment docs for today's gate.

## Original Contract Findings

| # | Sev | Status | Title | Fix |
|---|-----|--------|-------|-----|
| 3 | 🟠 high | confirmed | settle_contest pays each winner to a fully unconstrained destination ATA — prize pool redirectable to any USDC account | small |
| 5 | 🟠 high | confirmed | set_contest_lock_time / set_contest_conclusion_time (1-of-3) can re-open a locked contest or clear its conclusion, enabling late results-known entries | small |
| 6 | 🟡 medium | confirmed | settle_contest has no lock/conclusion-time precondition — a contest can be graded while still open for entries and before it concludes | small |
| 9 | 🟠 high | confirmed | mint_entry_token forges spendable free entries at 1-of-3 and has no on-chain source_ref idempotency despite docs claiming it | medium |
| 19 | 🔵 low | confirmed | Operator revenue commingled in one per-mint ATA; per-contest entry_fees tallies are advisory and can drift from real balances | medium |
| 25 | ⚪ info | uncertain | settle_contest remaining-account user/entry PDAs not checked for program ownership before deserialize/mutation | trivial |
| 26 | ⚪ info | uncertain | entry_token is not PDA-seed-bound in enter_contest_with_token — validated only by a self-asserted owner field | small |
| 28 | ⚪ info | confirmed | Mainnet build hard-codes declare_id! to the System Program ID placeholder | trivial |
| 29 | 🔵 low | confirmed | mint_entry_token idempotency keyed on caller-supplied sequence, not source_ref — operator-side double-mint of free entries | small |

### Contract-specific themes

- **Privilege tiering is inconsistent.** Value-forging / fund-fairness ops (`mint_entry_token`, `set_contest_lock_time`, `set_contest_conclusion_time`) sit at **1-of-3** (always-online Alex Bot key), while only settle/cancel/sweep require 2-of-3. A single compromised server key forges spendable value AND manipulates contest timing.
- **`settle_contest` validates most accounts but not the security-critical one** — the payout destination ATA is unconstrained, the same omission `sweep_operator_revenue` and `cancel_contest` avoid.
- **No enforced time boundary ties 'entries closed' to 'grading allowed'** — `Locked` status is vestigial, settle ignores lock/conclusion timestamps, and both timestamps are freely re-settable to past/zero values.

---

### 🟠 #3 — settle_contest pays each winner to a fully unconstrained destination ATA — prize pool redirectable to any USDC account

- **Severity:** HIGH · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** small
- **Location:** `programs/turf_vault/src/instructions/settle_contest.rs:128-173 (winner_ata = remaining[i*3+2], used verbatim at :164 with no owner/mint/ATA-derivation check)`

**Attack scenario**

Settlement remaining_accounts are triples [user_account, contest_entry, winner_ata]. The handler PDA-verifies the first two (user_account against [b'user', wallet] :134-141; contest_entry against [b'entry', contest_id, wallet, entry_num] :144-156) but NEVER validates the third — it is passed straight into the SPL Transfer as to: winner_ata_info.clone(). The only implicit constraint is the token program requiring winner_ata.mint == prize_pool.mint (USDC). Whoever assembles a settle TX (a malicious 2-of-3 insider, a compromised Alex-Bot server key + one human cosign, or a tamper of the remaining_accounts list before a human blind-signs in Phantom) keeps the real wallet/entry_num in the Settlement struct (so on-chain stats credit innocent winners) while substituting attacker-controlled USDC ATAs in slot 3, draining up to the full prize_pool. Contest then flips to Settled, making theft final. sweep_operator_revenue.rs DOES enforce treasury_ata.owner == treasury_authority — settle omits exactly that check, contradicting v0.16-spec §3.12 step 7 ('verify PDA seeds of all 3 triples').

**Impact**

Redirection of an entire contest's prize pool to an arbitrary account with on-chain stat counters left looking legitimate. Bounded by prize_pool per contest, unbounded across contests. Also turns any Rails bug in destination-ATA assembly into silent fund misrouting rather than a failed TX. High (not critical) because of the 2-of-3 settle precondition. (Merges TV-1 and FUND-1 — same root cause.)

**Fix (small)**

Bind the destination on-chain: require winner_ata == anchor_spl::associated_token::get_associated_token_address(&settlement.wallet, &payout_mint.key()), or unpack the destination TokenAccount and require owner == settlement.wallet AND mint == vault_state.payout_mint. This restores the spec's 'verify all 3 triples' intent and makes the cosigners' approval meaningful even under a tampered/blind-signed TX, matching the discipline already present in cancel_contest.rs and sweep_operator_revenue.rs.

---

### 🟠 #5 — set_contest_lock_time / set_contest_conclusion_time (1-of-3) can re-open a locked contest or clear its conclusion, enabling late results-known entries

- **Severity:** HIGH · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** small
- **Location:** `programs/turf_vault/src/instructions/set_contest_lock_time.rs:38-64; set_contest_conclusion_time.rs:36-60; create_contest.rs:152 (conclusion_timestamp defaults to 0)`

**Attack scenario**

create_contest hardcodes conclusion_timestamp = 0; setting one is a separate optional 1-of-3 call Rails may never make. The lock is purely time-derived (enter_contest rejects only when Clock &gt;= lock_timestamp; Locked status is vestigial, so a time-locked contest is still status==Open). A holder of any single 1-of-3 signer (the always-online Alex Bot server key, which signs set_contest_lock_time alone with no cosign per vault.rb:788-808) can: (1) call set_contest_lock_time with new_lock_timestamp = now + 1 week on an already-locked contest — the only guard is `if conclusion_timestamp != 0 { require now &lt; conclusion }`, skipped by the default 0, and there is no monotonicity/already-locked check — re-opening entries; (2) even if a conclusion was set to harden it, set_contest_conclusion_time lets the same key push it into the future or clear it to 0 any time before it passes, re-arming the relock; (3) timestamps also accept past/negative values (STATE-4) so a contest can be created already-locked or have its lock permanently bricked. The attacker then enters known-winning lineups after real-world results and is graded/paid on settle.

**Impact**

A single 1-of-3 / server-key holder can resurrect a locked contest and accept post-result entries (classic late-entry fraud), or grief contests by locking out all entries. Defeats the entire purpose of the derived lock and the v0.18 conclusion 'finality' guarantee. Requires a vault-signer key (not external), but a routine-tier 1-of-3 key defeats a fund-fairness control. (Merges STATE-2, STATE-3, STATE-4 — same root cause: 1-of-3 mutable, non-monotonic, unbounded lock/conclusion setters.)

**Fix (small)**

Make the lock effectively one-way: reject set_contest_lock_time once the contest has locked (lock_timestamp != 0 && now &gt;= lock_timestamp) unless raised to 2-of-3. Make conclusion strictly monotonic-forward and irrevocable (reject 0/clearing and any earlier value once set), and require 2-of-3 to extend a lock or change a conclusion since these have direct fund-fairness impact. Validate timestamps: require new_ts == 0 || new_ts &gt; Clock::now, reject negatives, and require lock &lt; conclusion when both are set. Do not rely on an opt-in conclusion_timestamp that defaults to 0.

---

### 🟡 #6 — settle_contest has no lock/conclusion-time precondition — a contest can be graded while still open for entries and before it concludes

- **Severity:** MEDIUM · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** small
- **Location:** `programs/turf_vault/src/instructions/settle_contest.rs:60-67 (status-only constraint), :90-216 (handler has no Clock/lock/conclusion check); enter_contest.rs:59,130-139; settle_contest.rs:216`

**Attack scenario**

settle_contest gates only on status == Open||Locked, with no Clock::get(), no lock_timestamp, and no conclusion_timestamp check. Because Locked is vestigial, every live contest is Open until settle flips it to Settled. With lock_timestamp == 0 (the create default unless Rails sets one), there is no on-chain lock at all, so entries are accepted up to the instant settle lands. There is thus a live race / no on-chain 'entries closed before grading' invariant: an entry can be slipped in immediately before grading, and the 2-of-3 settle authority can grade a contest before its lock/conclusion has passed. Note: payouts come from an operator-authored, 2-of-3-signed settlement vec built off-chain, so a brand-new on-chain entry not in that vec gets 0 (self-harm) — the on-chain gap alone does not extract funds.

**Impact**

Premature/early settlement and entry-after-effective-close races: the v0.18 conclusion-timestamp safety story is non-binding for the one operation it was meant to protect (final grading). Real harm is a defense-in-depth gap that lets a Rails bug or a partially-compromised signer grade a contest before it is over, or orphan a last-second entry (fee captured to op_rev, no payout). Bounded because the settlement set is operator-authored and multisig-gated. (Merges STATE-1 and STATE-5.)

**Fix (small)**

Gate settle on a passed lock/conclusion timestamp so there is a hard on-chain 'entries closed' boundary before grading: require!(contest.lock_timestamp != 0 && Clock::get()?.unix_timestamp &gt;= contest.lock_timestamp) (or conclusion_timestamp). Tie 'grading is allowed' to the same derived-time primitive the rest of v0.17/v0.18 relies on; consider snapshotting current_entries at lock and rejecting settle if entries changed after the lock boundary. This is the same fix that hardens rank 5.

---

### 🟠 #9 — mint_entry_token forges spendable free entries at 1-of-3 and has no on-chain source_ref idempotency despite docs claiming it

- **Severity:** HIGH · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** medium
- **Location:** `programs/turf_vault/src/instructions/mint_entry_token.rs:26-82; lib.rs:227-234; state.rs:299-313`

**Attack scenario**

mint_entry_token is gated at only 1-of-3 (is_signer, not validate_multisig), while comparable value ops (settle/cancel/sweep) require 2-of-3. user_wallet is an UncheckedAccount, so a single signer (the hot Alex Bot server key on Heroku) can mint a token to any wallet. The PDA is keyed solely on [b'entry_token', user_wallet, sequence] with sequence caller-supplied; source_ref is stored verbatim with NO uniqueness check, so CLAUDE.md's 'idempotent per source_ref' claim is false on-chain — re-minting the same Stripe source_ref with a fresh sequence creates a brand-new spendable token. An attacker holding the Alex Bot key mints unlimited unconsumed tokens, each redeemable into any open contest via enter_contest_with_token (owner + !consumed are the only gates).

**Impact**

Free-entry forgery at scale gated by a single server-held key rather than a 2-of-3 quorum. The doc/code idempotency mismatch means the operator pipeline (TokenPurchaseJob retries on the same source_ref) is protected only by off-chain DB resume logic, not the chain. High because exploitation requires possession of a vault signer key (not external); one reachability verdict noted that post-key-compromise this is marginal versus that key's other powers, but the 1-of-3 privilege-tiering and false-idempotency defects are genuine and key-theft-independent.

**Fix (medium)**

(1) Enforce on-chain idempotency: derive the PDA from a hash of source_ref (seeds [b'entry_token', source_ref_hash]) so re-minting the same external reference collides on init and fails. (2) Raise mint_entry_token to 2-of-3, or split out a dedicated lower-trust minter key with a per-period cap, since it directly forges spendable value. (3) Correct the CLAUDE.md 'idempotent per source_ref' claim, which is currently false.

---

### 🔵 #19 — Operator revenue commingled in one per-mint ATA; per-contest entry_fees tallies are advisory and can drift from real balances

- **Severity:** LOW · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** medium
- **Location:** `programs/turf_vault/src/instructions/enter_contest.rs:91-99,173-175; sweep_operator_revenue.rs:41-110; close_contest.rs:60-92`

**Attack scenario**

All entry fees for a mint across all contests flow into a single [b'op_rev', mint] PDA; close_contest (1-of-3) sweeps Settled-contest prize-pool dust into that same op_rev ATA; sweep_operator_revenue drains by live balance with no reference to per-contest entry_fees, and there is no global swept-revenue counter or on-chain invariant linking the commingled balance to the per-contest counters. prize_pool is never decremented on settle. So per-contest entry_fees[idx] can show already-swept revenue, and the numbers Rails uses for accounting are advisory only.

**Impact**

No theft of user principal (settle caps payouts at prize_pool, cancel refunds full balance, close moves only operator-margin dust from already-Settled contests). Pure accounting-trust limitation plus a minor threat-model note that a single 1-of-3 close can route Settled-contest residual into sweepable revenue (bounded because reaching Settled already required a 2-of-3 settle).

**Fix (medium)**

Document and reconcile: have sweep/close emit the per-contest delta, track a global swept-revenue counter on VaultState, and make the 1-of-3 authority on a fund-moving close explicit in the threat model. Confirm Cancelled contests cannot reach close with residual (cancel zeroes the pool first).

---

### ⚪ #25 — settle_contest remaining-account user/entry PDAs not checked for program ownership before deserialize/mutation

- **Severity:** INFO · **Status:** uncertain · **Component:** `turf-vault` · **Fix effort:** trivial
- **Location:** `programs/turf_vault/src/instructions/settle_contest.rs:129-213`

**Attack scenario**

settle passes user_account and contest_entry as raw AccountInfo and mutates via try_borrow_mut_data + try_serialize, verifying the address against find_program_address but with no explicit require!(info.owner == ctx.program_id). The discriminator check in try_deserialize plus the PDA-address pin are the only implicit guards. UNCERTAIN/effectively non-exploitable: the instruction is reachable only by the trusted 2-of-3 multisig, the PDA address is pinned, and the Solana runtime only persists writes to program-owned accounts — so a write-back to a forged non-program-owned account cannot commit.

**Impact**

No reproducible impact (trusted caller, address-pinned, runtime ownership enforcement on writes). A standard-Anchor remaining-account hygiene gap worth closing as defense-in-depth, but no fund/correctness consequence.

**Fix (trivial)**

Add explicit require!(user_account_info.owner == ctx.program_id, VaultError::Unauthorized) and the same for entry_account_info before deserializing, matching standard Anchor remaining-account hygiene.

---

### ⚪ #26 — entry_token is not PDA-seed-bound in enter_contest_with_token — validated only by a self-asserted owner field

- **Severity:** INFO · **Status:** uncertain · **Component:** `turf-vault` · **Fix effort:** small
- **Location:** `programs/turf_vault/src/instructions/enter_contest_with_token.rs:69-76`

**Attack scenario**

entry_token is the only account in the struct with no seeds constraint — accepted purely via Anchor program-ownership + discriminator, entry_token.owner == user.key(), and !entry_token.consumed. The PDA seeds [b'entry_token', owner, sequence] are never re-derived. UNCERTAIN/effectively non-exploitable today: such an account can only be created by the vault-signer-gated mint_entry_token, so an external attacker cannot forge one; the existing guards fully constrain behavior. It would become exploitable only if a future re-init or account-confusion bug let a user influence the owner field.

**Impact**

Defense-in-depth gap: the consumed-token guarantee rests on a mutable on-chain field instead of cryptographic PDA derivation, with no second line of defense if the owner field is ever attacker-influenced. No live exploit; the only benign behavioral nuance is that a user may consume any of their own fungible tokens rather than the exact sequence Rails chose.

**Fix (small)**

PDA-seed-bind the entry_token: add an instruction arg sequence: u64 and constrain seeds = [b'entry_token', user.key().as_ref(), &sequence.to_le_bytes()], bump = entry_token.bump, removing reliance on the mutable owner field as the sole gate.

---

### ⚪ #28 — Mainnet build hard-codes declare_id! to the System Program ID placeholder

- **Severity:** INFO · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** trivial
- **Location:** `programs/turf_vault/src/lib.rs:46-47; also Anchor.toml [programs.mainnet] and scripts/squad.json`

**Attack scenario**

On a --features mainnet build, declare_id!('11111111111111111111111111111111') resolves the program ID to the System Program. The mainnet branch is gated behind a manual one-shot launch step; default/devnet/prod builds bind the real ID. Deploying at the System Program address is impossible and Anchor's build-time program-id self-check aborts on mismatch, so accidental shipment would brick the deploy rather than silently misbehave.

**Impact**

Process/hardening only: a bricked mainnet deploy at most if the launch checklist is skipped. Not attacker-triggerable and unreachable in any current build.

**Fix (trivial)**

Replace with the real mainnet program keypair pubkey as the final launch step, and add a CI/compile_error! guard that fails any --features mainnet build whose declare_id! still equals 1111...1111.

---

### 🔵 #29 — mint_entry_token idempotency keyed on caller-supplied sequence, not source_ref — operator-side double-mint of free entries

- **Severity:** LOW · **Status:** confirmed · **Component:** `turf-vault` · **Fix effort:** small
- **Location:** `programs/turf_vault/src/instructions/mint_entry_token.rs:44-82 (PDA seeds [b'entry_token', user_wallet, sequence]; source_ref stored but never used for uniqueness)`

**Attack scenario**

The PDA and Anchor init uniqueness key on (user_wallet, sequence); source_ref is a stored field never used for uniqueness. Two mints with the same Stripe source_ref but different sequence both succeed, each producing a redeemable EntryTokenAccount. If Rails recomputes sequence for an already-fulfilled purchase (a retried/duplicated TokenPurchaseJob or webhook replay racing on per-wallet current_count), the same paid purchase yields two free-entry vouchers. Not externally reachable (requires the 1-of-3 admin key). Closely related to rank 9 (same false 'idempotent per source_ref' claim).

**Impact**

Operator-side double-issuance of free entries if the off-chain sequence allocation is not strictly idempotent. No fund theft; value forgery of paid entries, bounded by admin-key trust + off-chain sequence discipline since there is no on-chain idempotency backstop.

**Fix (small)**

Derive the PDA from a hash of source_ref (so a given external reference mints at most one token), or document that idempotency is Rails' responsibility via a strictly monotonic per-wallet sequence plus a DB unique index on source_ref. Update the misleading 'idempotent per source_ref' comments. (Same remediation cluster as rank 9.)

---
