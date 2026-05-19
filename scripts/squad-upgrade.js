#!/usr/bin/env node
// OPSEC-002: deploy a turf-vault upgrade THROUGH the Squads 2-of-3 multisig.
//
// Since 2026-05-19 the program upgrade authority is the Squad vault
// (see scripts/squad.json) — `anchor deploy` no longer works. The full
// upgrade flow:
//
//   1. anchor build
//   2. solana program write-buffer target/deploy/turf_vault.so --url devnet
//        → prints "Buffer: <ADDR>"
//   3. solana program set-buffer-authority <BUFFER> \
//        --new-buffer-authority <vaultPda from squad.json> --url devnet
//   4. ALEX_BOT_KEY=... MASON_KEY=... node scripts/squad-upgrade.js <BUFFER>
//
// Step 4 wraps the BPF Loader `upgrade` instruction in a Squad vault
// transaction and runs propose → approve(Alex Bot) → approve(Mason) →
// execute. Any two of the three members can approve; this script uses
// Alex Bot + Mason because their keys are in 1Password (agent.solana,
// agent.mason.solana). To cosign with Alex instead, adapt the approver.
//
// Keys are passed via env (never argv — argv leaks in `ps`):
//   ALEX_BOT_KEY  base58 secret  (creator + approver #1 + fee payer)
//   MASON_KEY     base58 secret  (approver #2)

const multisig = require("@sqds/multisig");
const {
  Connection, Keypair, PublicKey, TransactionMessage, TransactionInstruction,
} = require("@solana/web3.js");
const bs58 = require("bs58").default;
const fs = require("fs");
const path = require("path");

const cfg = JSON.parse(fs.readFileSync(path.join(__dirname, "squad.json"), "utf8"));
const RPC = process.env.SOLANA_RPC_URL ||
  (cfg.network === "mainnet-beta" ? "https://api.mainnet-beta.solana.com" : "https://api.devnet.solana.com");

const BPF_LOADER  = new PublicKey("BPFLoaderUpgradeab1e11111111111111111111111");
const SYSVAR_RENT = new PublicKey("SysvarRent111111111111111111111111111111111");
const SYSVAR_CLOCK = new PublicKey("SysvarC1ock11111111111111111111111111111111");

function loadKey(env) {
  const v = process.env[env];
  if (!v) throw new Error(`${env} env var not set (base58 secret key)`);
  return Keypair.fromSecretKey(bs58.decode(v.trim()));
}
async function confirm(connection, sig, label) {
  const s = typeof sig === "string" ? sig : sig.signature;
  const bh = await connection.getLatestBlockhash();
  await connection.confirmTransaction({ signature: s, ...bh }, "confirmed");
  console.log(`  ${label}: ${s}`);
}

(async () => {
  const bufferArg = process.argv[2];
  if (!bufferArg) throw new Error("usage: node scripts/squad-upgrade.js <BUFFER_ADDRESS>");

  const connection = new Connection(RPC, "confirmed");
  const program   = new PublicKey(cfg.programId);
  const multisigPda = new PublicKey(cfg.multisigPda);
  const vaultPda  = new PublicKey(cfg.vaultPda);
  const buffer    = new PublicKey(bufferArg);
  const alexBot   = loadKey("ALEX_BOT_KEY");
  const mason     = loadKey("MASON_KEY");

  // ProgramData PDA is deterministic: [programId] under the BPF loader.
  const [programData] = PublicKey.findProgramAddressSync([program.toBuffer()], BPF_LOADER);

  console.log("turf-vault Squad upgrade");
  console.log("  program:    ", program.toBase58());
  console.log("  programData:", programData.toBase58());
  console.log("  buffer:     ", buffer.toBase58());
  console.log("  squad vault:", vaultPda.toBase58());

  // Sanity: buffer authority must already be the Squad vault (step 3 above).
  const bufInfo = await connection.getAccountInfo(buffer);
  if (!bufInfo) throw new Error(`buffer ${buffer.toBase58()} not found — did write-buffer succeed?`);

  const upgradeIx = new TransactionInstruction({
    programId: BPF_LOADER,
    keys: [
      { pubkey: programData,       isSigner: false, isWritable: true },
      { pubkey: program,           isSigner: false, isWritable: true },
      { pubkey: buffer,            isSigner: false, isWritable: true },
      { pubkey: alexBot.publicKey, isSigner: false, isWritable: true }, // spill — rent refund
      { pubkey: SYSVAR_RENT,       isSigner: false, isWritable: false },
      { pubkey: SYSVAR_CLOCK,      isSigner: false, isWritable: false },
      { pubkey: vaultPda,          isSigner: true,  isWritable: false }, // upgrade authority
    ],
    data: Buffer.from([3, 0, 0, 0]), // BPF Loader Upgradeable: Upgrade
  });

  const ms = await multisig.accounts.Multisig.fromAccountAddress(connection, multisigPda);
  const txIndex = multisig.utils.toBigInt(ms.transactionIndex) + 1n;
  console.log("  vault tx index:", txIndex.toString());

  const innerMessage = new TransactionMessage({
    payerKey: vaultPda,
    recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
    instructions: [upgradeIx],
  });

  await confirm(connection, await multisig.rpc.vaultTransactionCreate({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex,
    creator: alexBot.publicKey, vaultIndex: 0, ephemeralSigners: 0,
    transactionMessage: innerMessage,
  }), "vaultTransactionCreate");

  await confirm(connection, await multisig.rpc.proposalCreate({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, creator: alexBot,
  }), "proposalCreate");

  await confirm(connection, await multisig.rpc.proposalApprove({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, member: alexBot,
  }), "approve (Alex Bot)");

  await confirm(connection, await multisig.rpc.proposalApprove({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, member: mason,
  }), "approve (Mason)");

  await confirm(connection, await multisig.rpc.vaultTransactionExecute({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex,
    member: alexBot.publicKey, computeUnitLimit: 400_000, computeUnitPrice: 1_000,
  }), "vaultTransactionExecute (upgrade)");

  console.log("\nUpgrade executed. Verify:");
  console.log(`  solana program show ${program.toBase58()} --url ${cfg.network}`);
})().catch((e) => {
  console.error("FAILED:", e.message);
  if (e.logs) console.error(e.logs.join("\n"));
  process.exit(1);
});
