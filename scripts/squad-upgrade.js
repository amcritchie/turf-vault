#!/usr/bin/env node
// OPSEC-002: deploy a turf-vault upgrade THROUGH the Squads 2-of-3 multisig.
//
// Since 2026-05-19 the program upgrade authority is the Squad vault
// (see scripts/squad.json) — `anchor deploy` no longer works. Flow:
//
//   1. anchor build
//   2. solana program write-buffer target/deploy/turf_vault.so --url devnet
//        → prints "Buffer: <ADDR>"
//   3. solana program set-buffer-authority <BUFFER> \
//        --new-buffer-authority <vaultPda from squad.json> --url devnet
//   4. ALEX_BOT_KEY=... MASON_KEY=... node scripts/squad-upgrade.js <BUFFER>
//
// Step 4: (a) extends the ProgramData account if the new binary is larger
// than the current one — the BPF `upgrade` instruction does NOT auto-grow
// it the way `solana program deploy` does, and ExtendProgram is
// permissionless so we do it directly; (b) wraps the BPF `upgrade`
// instruction in a Squad vault transaction → propose → approve(Alex Bot)
// → approve(Mason) → execute.
//
// Keys via env (never argv — argv leaks in `ps`):
//   ALEX_BOT_KEY  base58 secret  (creator + approver #1 + fee payer)
//   MASON_KEY     base58 secret  (approver #2)

const multisig = require("@sqds/multisig");
const {
  Connection, Keypair, PublicKey, TransactionMessage, TransactionInstruction,
  VersionedTransaction, ComputeBudgetProgram, SystemProgram,
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

// Account-data overheads (UpgradeableLoaderState serialized sizes).
const BUFFER_HEADER = 37;
const PROGRAMDATA_HEADER = 45;

function loadKey(env) {
  const v = process.env[env];
  if (!v) throw new Error(`${env} env var not set (base58 secret key)`);
  return Keypair.fromSecretKey(bs58.decode(v.trim()));
}
async function sendAndConfirm(connection, ixs, signers, label) {
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  const msg = new TransactionMessage({
    payerKey: signers[0].publicKey, recentBlockhash: blockhash, instructions: ixs,
  }).compileToV0Message();
  const vtx = new VersionedTransaction(msg);
  vtx.sign(signers);
  try {
    const sig = await connection.sendTransaction(vtx, { skipPreflight: false });
    await connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
    console.log(`  ${label}: ${sig}`);
    return sig;
  } catch (e) {
    console.error(`  ${label} FAILED: ${e.message}`);
    const sim = await connection.simulateTransaction(vtx, { sigVerify: false }).catch(() => null);
    if (sim?.value?.logs) { console.error("  --- logs ---"); sim.value.logs.forEach((l) => console.error("  " + l)); }
    throw e;
  }
}
async function confirmSig(connection, sig, label) {
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

  const [programData] = PublicKey.findProgramAddressSync([program.toBuffer()], BPF_LOADER);

  console.log("turf-vault Squad upgrade");
  console.log("  program:    ", program.toBase58());
  console.log("  programData:", programData.toBase58());
  console.log("  buffer:     ", buffer.toBase58());
  console.log("  squad vault:", vaultPda.toBase58());

  const bufInfo = await connection.getAccountInfo(buffer);
  if (!bufInfo) throw new Error(`buffer ${buffer.toBase58()} not found — did write-buffer succeed?`);
  const pdInfo = await connection.getAccountInfo(programData);
  if (!pdInfo) throw new Error(`programData ${programData.toBase58()} not found`);

  // --- Extend ProgramData if the new binary won't fit -----------------------
  // The Squad upgrade only runs the BPF `upgrade` instruction, which (unlike
  // `solana program deploy`) does NOT grow ProgramData. ExtendProgram is
  // permissionless — do it directly, paid by Alex Bot.
  const needed = (bufInfo.data.length - BUFFER_HEADER) + PROGRAMDATA_HEADER;
  if (needed > pdInfo.data.length) {
    const additional = needed - pdInfo.data.length + 1024; // +1KB margin
    console.log(`\nProgramData too small (${pdInfo.data.length} < ${needed}) — extending by ${additional} bytes...`);
    const extendData = Buffer.alloc(8);
    extendData.writeUInt32LE(6, 0);          // ExtendProgram variant
    extendData.writeUInt32LE(additional, 4); // additional_bytes
    const extendIx = new TransactionInstruction({
      programId: BPF_LOADER,
      keys: [
        { pubkey: programData,        isSigner: false, isWritable: true },
        { pubkey: program,            isSigner: false, isWritable: true },
        { pubkey: SystemProgram.programId, isSigner: false, isWritable: false },
        { pubkey: alexBot.publicKey,  isSigner: true,  isWritable: true },
      ],
      data: extendData,
    });
    await sendAndConfirm(connection, [extendIx], [alexBot], "extendProgram");
  } else {
    console.log("\nProgramData already large enough — no extend needed.");
  }

  // --- Squad vault transaction: the BPF `upgrade` instruction ---------------
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
    data: Buffer.from([3, 0, 0, 0]), // Upgrade
  });

  const ms = await multisig.accounts.Multisig.fromAccountAddress(connection, multisigPda);
  const txIndex = multisig.utils.toBigInt(ms.transactionIndex) + 1n;
  console.log("\nSquad vault tx index:", txIndex.toString());

  const innerMessage = new TransactionMessage({
    payerKey: vaultPda,
    recentBlockhash: (await connection.getLatestBlockhash()).blockhash,
    instructions: [upgradeIx],
  });

  await confirmSig(connection, await multisig.rpc.vaultTransactionCreate({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex,
    creator: alexBot.publicKey, vaultIndex: 0, ephemeralSigners: 0, transactionMessage: innerMessage,
  }), "vaultTransactionCreate");
  await confirmSig(connection, await multisig.rpc.proposalCreate({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, creator: alexBot,
  }), "proposalCreate");
  await confirmSig(connection, await multisig.rpc.proposalApprove({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, member: alexBot,
  }), "approve (Alex Bot)");
  await confirmSig(connection, await multisig.rpc.proposalApprove({
    connection, feePayer: alexBot, multisigPda, transactionIndex: txIndex, member: mason,
  }), "approve (Mason)");

  // Execute via build-then-send so an on-chain failure surfaces real logs
  // (multisig.rpc.vaultTransactionExecute swallows them behind a web3.js
  // error-wrapping bug).
  const { instruction: execIx, lookupTableAccounts } =
    await multisig.instructions.vaultTransactionExecute({
      connection, multisigPda, transactionIndex: txIndex, member: alexBot.publicKey,
    });
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash();
  const execMsg = new TransactionMessage({
    payerKey: alexBot.publicKey, recentBlockhash: blockhash,
    instructions: [
      ComputeBudgetProgram.setComputeUnitLimit({ units: 1_400_000 }),
      ComputeBudgetProgram.setComputeUnitPrice({ microLamports: 5_000 }),
      execIx,
    ],
  }).compileToV0Message(lookupTableAccounts);
  const execTx = new VersionedTransaction(execMsg);
  execTx.sign([alexBot]);
  try {
    const sig = await connection.sendTransaction(execTx, { skipPreflight: false });
    await connection.confirmTransaction({ signature: sig, blockhash, lastValidBlockHeight }, "confirmed");
    console.log("  vaultTransactionExecute (upgrade):", sig);
  } catch (e) {
    console.error("  execute FAILED:", e.message);
    const sim = await connection.simulateTransaction(execTx, { sigVerify: false }).catch(() => null);
    if (sim?.value?.logs) { console.error("  --- logs ---"); sim.value.logs.forEach((l) => console.error("  " + l)); }
    process.exit(1);
  }

  console.log("\nUpgrade executed. Verify:");
  console.log(`  solana program show ${program.toBase58()} --url ${cfg.network}`);
})().catch((e) => { console.error("FAILED:", e.message); process.exit(1); });
