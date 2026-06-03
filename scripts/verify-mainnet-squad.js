#!/usr/bin/env node
// READ-ONLY verification of the new mainnet "Turf Totals" Squads V4 multisig.
// No transactions. Walks the vault PDA → finds the multisig config → decodes
// members/threshold/configAuthority via @sqds/multisig.

const multisig = require("@sqds/multisig");
const { Connection, PublicKey } = require("@solana/web3.js");

const RPC = process.env.SOLANA_RPC_URL;
if (!RPC) throw new Error("SOLANA_RPC_URL not set");

const SQDS_V4 = new PublicKey("SQDS4ep65T869zMMBKyuUq6aD6EgTu8psMjkvj52pCf");
const VAULT_PDA = new PublicKey("Bk9sS7iiSRL18vuo2KVzkeGw7EekKqxMCjrdoyGGdJm");

const NEW_BOT = "8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd";
const ALEX    = "7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr";
const MASON   = "CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR";
const LEAKED  = "F6f8h5yynbnkgWvU5abQx3RJxJpe8EoQmeFBuNKdKzhZ";

const EXPECTED = new Set([NEW_BOT, ALEX, MASON]);

(async () => {
  const c = new Connection(RPC, "confirmed");

  // 1. Confirm vault is System-owned + dust (Squads vault PDAs are SystemProgram-owned).
  const vinfo = await c.getAccountInfo(VAULT_PDA);
  const vaultBalanceLamports = vinfo ? vinfo.lamports : 0;
  console.log("=== VAULT ACCOUNT ===");
  console.log("vaultPda:", VAULT_PDA.toBase58());
  console.log("owner:", vinfo ? vinfo.owner.toBase58() : "(account not found / 0 lamports)");
  console.log("lamports:", vaultBalanceLamports, "=", (vaultBalanceLamports / 1e9).toFixed(9), "SOL");
  console.log("dataLen:", vinfo ? vinfo.data.length : "n/a");

  // 2. Walk the vault's signature history to find its creation tx, then
  //    extract the multisig config account from the multisigCreate ix.
  //    But the deterministic path is: scan all SQDS V4 multisig accounts and
  //    test getVaultPda(config, 0) == VAULT_PDA. We avoid a full getProgramAccounts
  //    scan (heavy) by deriving from the creation tx instead.
  console.log("\n=== LOCATING MULTISIG CONFIG ===");
  let configPda = null;

  // Fetch oldest signatures for the vault PDA (its funding/creation history).
  const sigs = await c.getSignaturesForAddress(VAULT_PDA, { limit: 1000 });
  console.log("vault signature count (≤1000):", sigs.length);
  const oldest = sigs[sigs.length - 1];
  if (oldest) {
    const tx = await c.getTransaction(oldest.signature, {
      maxSupportedTransactionVersion: 0,
      commitment: "confirmed",
    });
    const keys = tx
      ? tx.transaction.message.staticAccountKeys ||
        tx.transaction.message.accountKeys
      : [];
    // Candidate config = any account in the oldest tx whose getVaultPda(_,0) matches.
    for (const k of keys) {
      try {
        const [vp] = multisig.getVaultPda({
          multisigPda: new PublicKey(k),
          index: 0,
          programId: SQDS_V4,
        });
        if (vp.equals(VAULT_PDA)) {
          configPda = new PublicKey(k);
          console.log("found config via creation tx:", configPda.toBase58());
          break;
        }
      } catch (_) {}
    }
  }

  // Fallback: getProgramAccounts on SQDS V4, filter by Multisig discriminator,
  // test each. Only if the tx walk failed.
  if (!configPda) {
    console.log("creation-tx walk did not yield config; scanning SQDS V4 multisig accounts...");
    const accts = await c.getProgramAccounts(SQDS_V4, {
      filters: [{ memcmp: { offset: 0, bytes: multisig.accounts.Multisig.discriminator ? undefined : undefined } }].filter(Boolean),
    }).catch(() => []);
    for (const { pubkey } of accts) {
      try {
        const [vp] = multisig.getVaultPda({ multisigPda: pubkey, index: 0, programId: SQDS_V4 });
        if (vp.equals(VAULT_PDA)) { configPda = pubkey; break; }
      } catch (_) {}
    }
    if (configPda) console.log("found config via scan:", configPda.toBase58());
  }

  if (!configPda) {
    console.log("\n✗ COULD NOT LOCATE MULTISIG CONFIG for this vault. Aborting decode.");
    process.exit(2);
  }

  // 3. Confirm derivation explicitly.
  const [derivedVault, bump] = multisig.getVaultPda({
    multisigPda: configPda, index: 0, programId: SQDS_V4,
  });
  console.log("\n=== DERIVATION CHECK ===");
  console.log("getVaultPda(config, 0) =", derivedVault.toBase58(), "bump", bump);
  console.log("matches target vault:", derivedVault.equals(VAULT_PDA) ? "YES ✓" : "NO ✗");

  // 4. Confirm the config account is owned by SQDS V4, then decode it.
  const cinfo = await c.getAccountInfo(configPda);
  console.log("\n=== CONFIG ACCOUNT ===");
  console.log("multisigPda:", configPda.toBase58());
  console.log("owner:", cinfo ? cinfo.owner.toBase58() : "(not found)");
  console.log("owned by SQDS V4:", cinfo && cinfo.owner.equals(SQDS_V4) ? "YES ✓" : "NO ✗");

  const ms = await multisig.accounts.Multisig.fromAccountAddress(c, configPda);
  console.log("\n=== DECODED MULTISIG ===");
  console.log("threshold:", ms.threshold.toString());
  console.log("configAuthority:", ms.configAuthority.toBase58());
  console.log("timeLock:", ms.timeLock);
  console.log("staleTransactionIndex:", ms.staleTransactionIndex.toString());
  console.log("members count:", ms.members.length);
  console.log("\nMembers:");
  for (const m of ms.members) {
    console.log("  key:", m.key.toBase58(), " permissions.mask:", m.permissions.mask);
  }

  // ---- VERDICT ----
  console.log("\n=== VERDICT ===");
  const memberKeys = ms.members.map((m) => m.key.toBase58());
  const hasLeaked = memberKeys.includes(LEAKED);
  const setMatch =
    memberKeys.length === 3 &&
    memberKeys.every((k) => EXPECTED.has(k)) &&
    [...EXPECTED].every((k) => memberKeys.includes(k));

  // Squads permission masks: Initiate=1, Vote=2, Execute=4. Vote bit = 2.
  const allCanVote = ms.members.every((m) => (m.permissions.mask & 2) === 2);

  console.log("threshold == 2:", ms.threshold === 2 ? "✓" : `✗ (${ms.threshold})`);
  console.log("exactly the 3 expected members:", setMatch ? "✓" : "✗");
  console.log("all members can VOTE (mask & 2):", allCanVote ? "✓" : "✗");
  console.log("configAuthority == None (default/system):",
    ms.configAuthority.equals(PublicKey.default) ? "✓ (proposal-governed)" : `✗ ${ms.configAuthority.toBase58()}`);
  console.log("LEAKED F6f8 present:", hasLeaked ? "✗✗✗ PRESENT — ABORT" : "✓ ABSENT");

  console.log("\n--- PASTE BLOCK ---");
  console.log("multisigPda =", configPda.toBase58());
  console.log("vaultPda    =", VAULT_PDA.toBase58());
  console.log("threshold   =", ms.threshold);
  console.log("members     =");
  for (const k of memberKeys) {
    let label = "UNKNOWN/UNEXPECTED";
    if (k === NEW_BOT) label = "new bot";
    else if (k === ALEX) label = "alex/human";
    else if (k === MASON) label = "mason";
    else if (k === LEAKED) label = "!!! LEAKED F6f8 !!!";
    console.log("  ", k, "(" + label + ")");
  }
})();
