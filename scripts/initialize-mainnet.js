#!/usr/bin/env node
// Mainnet first-time `initialize` call (runbook §7).
//
// Flow (assumes runbook §1-§6 are complete):
//   1. Mainnet binary deployed, upgrade authority transferred to Squads.
//   2. CLI keypair set to Alex Phantom (INIT_AUTHORITY — Alex is the only
//      key that can call initialize in a mainnet build per state.rs).
//   3. Alex Phantom has ≥ 0.05 SOL on mainnet for rent.
//   4. scripts/squad.json's `mainnet` block has been filled in (programId,
//      multisigPda, vaultPda) — placeholders refused.
//
// What this script does:
//   - Loads anchor's IDL + program ID from disk (target/idl/turf_vault.json +
//     scripts/squad.json mainnet.programId).
//   - Derives the three PDAs: vault_state ([b"vault"]), payout_op_rev_ata
//     ([b"op_rev", USDC]), second_op_rev_ata ([b"op_rev", USDT]).
//   - Builds + sends the initialize TX with:
//       signers          = [alex_bot, alex, mason] from squad.json
//       threshold        = 2
//       treasury_authority = Squads vault PDA
//       admin            = Alex Phantom (CLI keypair)
//       payout_mint      = canonical Circle USDC
//       second_currency_mint = canonical Tether USDT
//   - Verifies the resulting VaultState via accounts.fetch + prints the
//     summary the runbook §7 expects.
//
// Usage:
//   solana config set --keypair /path/to/alex-phantom-mainnet-keypair.json
//   node scripts/initialize-mainnet.js
//
// On failure, the on-chain account either doesn't exist yet (the TX errored
// before init) or partial state landed (the TX succeeded but the script's
// fetch failed). Either way: re-run is safe IFF the previous TX failed
// before vault_state was created — the `init` constraint will refuse a
// double-init. If vault_state exists, abort: something else is wrong.

const anchor = require("@coral-xyz/anchor");
const { Connection, PublicKey, Keypair, SystemProgram, SYSVAR_RENT_PUBKEY } = require("@solana/web3.js");
const { TOKEN_PROGRAM_ID } = require("@solana/spl-token");
const fs = require("fs");
const path = require("path");
const os = require("os");

const USDC_MAINNET_MINT = new PublicKey("EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v");
const USDT_MAINNET_MINT = new PublicKey("Es9vMFrzaCERmJfrF4H2FYD4KCoNkY11McCe8BenwNYB");

const PLACEHOLDER_PUBKEY = "11111111111111111111111111111111";

function abort(msg) {
  console.error(`\n[initialize-mainnet] ABORT: ${msg}\n`);
  process.exit(1);
}

function readJson(p) {
  return JSON.parse(fs.readFileSync(p, "utf8"));
}

function loadKeypair(filepath) {
  const raw = JSON.parse(fs.readFileSync(filepath, "utf8"));
  return Keypair.fromSecretKey(Uint8Array.from(raw));
}

(async () => {
  const cfgPath = path.join(__dirname, "squad.json");
  const cfg = readJson(cfgPath);

  if (!cfg.mainnet) abort(`squad.json has no 'mainnet' block — see runbook §3.`);
  const m = cfg.mainnet;

  // Refuse placeholder values (runbook §3 replaces these).
  for (const k of ["programId", "multisigPda", "vaultPda"]) {
    if (!m[k] || m[k] === PLACEHOLDER_PUBKEY) {
      abort(`squad.json mainnet.${k} is still the placeholder '${PLACEHOLDER_PUBKEY}'. Run runbook §3 first.`);
    }
  }
  for (const role of ["alex_bot", "alex", "mason"]) {
    if (!m.members?.[role]) abort(`squad.json mainnet.members.${role} is missing.`);
  }

  const programId       = new PublicKey(m.programId);
  const treasuryAuth    = new PublicKey(m.vaultPda);  // Squads vault PDA from runbook §2
  const signers         = [m.members.alex_bot, m.members.alex, m.members.mason].map((k) => new PublicKey(k));
  const threshold       = m.threshold ?? 2;

  // RPC: prefer SOLANA_RPC_URL (operator can use Helius). Fall back to public mainnet.
  const rpcUrl = process.env.SOLANA_RPC_URL || "https://api.mainnet-beta.solana.com";
  const connection = new Connection(rpcUrl, "confirmed");
  console.log(`[initialize-mainnet] RPC: ${rpcUrl.replace(/api-key=[^&]+/, "api-key=***")}`);
  console.log(`[initialize-mainnet] Program: ${programId.toBase58()}`);

  // CLI keypair = the signer for this TX. Must equal INIT_AUTHORITY in
  // the deployed mainnet binary (Alex Phantom).
  const cliKeyPath = process.env.SOLANA_ADMIN_KEYPAIR ||
    path.join(os.homedir(), ".config/solana/id.json");
  const adminKeypair = loadKeypair(cliKeyPath);
  console.log(`[initialize-mainnet] Admin (signer): ${adminKeypair.publicKey.toBase58()}`);
  console.log(`[initialize-mainnet] Treasury (Squads vault PDA): ${treasuryAuth.toBase58()}`);
  console.log(`[initialize-mainnet] Signers: ${signers.map((s) => s.toBase58()).join(", ")} (threshold ${threshold})`);

  // Anchor program — IDL emitted by `anchor build` at runbook §4.
  const idlPath = path.join(__dirname, "..", "target", "idl", "turf_vault.json");
  if (!fs.existsSync(idlPath)) abort(`IDL not found at ${idlPath}. Run \`anchor build -- --features mainnet\` (runbook §4) first.`);
  const idl = readJson(idlPath);

  const wallet = new anchor.Wallet(adminKeypair);
  const provider = new anchor.AnchorProvider(connection, wallet, { commitment: "confirmed" });
  const program = new anchor.Program(idl, provider);

  // Derive PDAs.
  const [vaultStatePda]      = PublicKey.findProgramAddressSync([Buffer.from("vault")], programId);
  const [payoutOpRevPda]     = PublicKey.findProgramAddressSync([Buffer.from("op_rev"), USDC_MAINNET_MINT.toBuffer()], programId);
  const [secondOpRevPda]     = PublicKey.findProgramAddressSync([Buffer.from("op_rev"), USDT_MAINNET_MINT.toBuffer()], programId);
  console.log(`[initialize-mainnet] VaultState PDA: ${vaultStatePda.toBase58()}`);
  console.log(`[initialize-mainnet] USDC op_rev PDA: ${payoutOpRevPda.toBase58()}`);
  console.log(`[initialize-mainnet] USDT op_rev PDA: ${secondOpRevPda.toBase58()}`);

  // Refuse if vault already exists (init constraint would fail anyway, but
  // a clean error is friendlier than an Anchor "AccountAlreadyInitialized").
  const existing = await connection.getAccountInfo(vaultStatePda);
  if (existing) abort(`VaultState already exists at ${vaultStatePda.toBase58()}. Vault is initialized — nothing to do.`);

  console.log(`\n[initialize-mainnet] Submitting initialize TX…`);
  const sig = await program.methods
    .initialize(signers, threshold, treasuryAuth)
    .accountsStrict({
      admin: adminKeypair.publicKey,
      vaultState: vaultStatePda,
      payoutMint: USDC_MAINNET_MINT,
      secondCurrencyMint: USDT_MAINNET_MINT,
      payoutOpRevAta: payoutOpRevPda,
      secondOpRevAta: secondOpRevPda,
      tokenProgram: TOKEN_PROGRAM_ID,
      systemProgram: SystemProgram.programId,
      rent: SYSVAR_RENT_PUBKEY,
    })
    .rpc({ commitment: "confirmed" });

  console.log(`[initialize-mainnet] TX signature: ${sig}`);
  console.log(`[initialize-mainnet]   solscan: https://solscan.io/tx/${sig}?cluster=mainnet-beta`);

  // Verify
  console.log(`\n[initialize-mainnet] Verifying VaultState…`);
  const vault = await program.account.vaultState.fetch(vaultStatePda);
  console.log(`  signers          : [${vault.signers.map((s) => s.toBase58()).join(", ")}]`);
  console.log(`  threshold        : ${vault.threshold}`);
  console.log(`  paused           : ${vault.paused}`);
  console.log(`  payout_mint      : ${vault.payoutMint.toBase58()}`);
  console.log(`  treasury_authority: ${vault.treasuryAuthority.toBase58()}`);
  console.log(`  accepted_currencies[0]: mint=${vault.acceptedCurrencies[0].mint.toBase58()} active=${vault.acceptedCurrencies[0].active}`);
  console.log(`  accepted_currencies[1]: mint=${vault.acceptedCurrencies[1].mint.toBase58()} active=${vault.acceptedCurrencies[1].active}`);

  // Sanity asserts — fail loud if anything's off.
  const ok =
    vault.payoutMint.toBase58() === USDC_MAINNET_MINT.toBase58() &&
    vault.treasuryAuthority.toBase58() === treasuryAuth.toBase58() &&
    vault.threshold === threshold &&
    vault.acceptedCurrencies[0].active === 1 &&
    vault.acceptedCurrencies[1].active === 1;
  if (!ok) abort(`VaultState verification failed — manual inspection required.`);

  console.log(`\n[initialize-mainnet] ✓ Vault initialized on mainnet. Continue at runbook §8.`);
})().catch((e) => abort(`${e.message}\n${e.stack ?? ""}`));
