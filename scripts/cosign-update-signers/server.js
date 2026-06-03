#!/usr/bin/env node
/* eslint-disable */
// =============================================================================
// DEVNET-ONLY local cosign server for the turf-vault `update_signers` rotation
//   F6f8 (leaked Alex Bot)  -->  8K81 (new Alex Bot)
//   New signer set: [8K81, 7ZDJ (Alex), Cyt (Mason)] — threshold pinned 2-of-3.
//
// 2-signer tx:
//   admin     = Mason  (Cyt…qWjrR)  — scriptable, partial-signed HERE
//   cosigner  = Alex   (7ZDJ…59Tcr) — signs in Phantom (operator action)
//   fee payer = Alex   (7ZDJ…59Tcr) — so Phantom can pay
//
// The 7ZDJ signature is the ONLY thing this server cannot produce. The operator
// supplies it via Phantom in the browser; this server just builds + partial-signs
// + broadcasts the fully-signed tx.
//
// Endpoints:
//   GET  /            -> the cosign page (no secrets, no RPC URL in the HTML)
//   POST /build       -> { txBase64 }  fresh blockhash, fee-payer 7ZDJ, Mason partial-signed
//   POST /rpc         -> JSON-RPC proxy to Helius DEVNET (URL stays server-side)
//
// Secrets (pulled from 1Password at startup; never in the page, never in argv):
//   Helius devnet URL : op://agents/agent.helius/Devnet RPC URL
//   Mason secret key  : op://agents/agent.mason.solana/private key
//
// Run (from anywhere):
//   node /Users/alex/projects/turf-vault/scripts/cosign-update-signers/server.js
// =============================================================================

const http = require("http");
const fs = require("fs");
const path = require("path");
const { execFileSync } = require("child_process");

const W3 = "/Users/alex/projects/turf-vault/node_modules/@solana/web3.js";
const BS58 = "/Users/alex/projects/turf-vault/node_modules/bs58";
const {
  Connection, Keypair, PublicKey, TransactionMessage,
  TransactionInstruction, VersionedTransaction,
} = require(W3);
const bs58 = require(BS58).default;

const PORT = 8799;

// --- Constants for THIS rotation (devnet) -----------------------------------
const PROGRAM   = new PublicKey("EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ");
const VAULT     = new PublicKey("J7b5g9uS5M2Nog1Ly1UATXTDMtXdpXK3JffRAHXGHkK2");
const ALEX_7ZDJ = new PublicKey("7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr"); // cosigner + fee payer
const MASON_CYT = new PublicKey("CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR"); // admin
const NEW_SIGNERS = [
  new PublicKey("8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd"),
  new PublicKey("7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr"),
  new PublicKey("CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR"),
];
// update_signers anchor discriminator (sha256("global:update_signers")[..8])
const DISC = Buffer.from("e45244965c428cae", "hex");

// --- Pull secrets from 1Password (stdout captured, never logged) ------------
function op(item, field) {
  return execFileSync(
    "op",
    ["item", "get", item, "--vault", "agents", "--fields", field, "--reveal"],
    { encoding: "utf8" }
  ).trim();
}

let HELIUS_URL, MASON;
try {
  HELIUS_URL = op("agent.helius", "Devnet RPC URL");
  MASON = Keypair.fromSecretKey(bs58.decode(op("agent.mason.solana", "private key")));
} catch (e) {
  console.error("FATAL: could not pull secrets from 1Password.");
  console.error("  Ensure OP_SERVICE_ACCOUNT_TOKEN is set (source ~/.zprofile).");
  console.error("  " + e.message);
  process.exit(1);
}
if (MASON.publicKey.toBase58() !== MASON_CYT.toBase58()) {
  console.error(`FATAL: Mason key resolved to ${MASON.publicKey.toBase58()}, expected ${MASON_CYT.toBase58()}`);
  process.exit(1);
}
if (!/devnet/i.test(HELIUS_URL)) {
  console.error(`FATAL: Helius URL is not a devnet URL — refusing to run. (${HELIUS_URL.slice(0, 40)}…)`);
  process.exit(1);
}
const connection = new Connection(HELIUS_URL, "confirmed");

// --- Build + Mason-partial-sign a FRESH tx ----------------------------------
async function buildPartialSigned() {
  const data = Buffer.concat([DISC, ...NEW_SIGNERS.map((k) => k.toBuffer())]);
  const ix = new TransactionInstruction({
    programId: PROGRAM,
    keys: [
      { pubkey: MASON_CYT, isSigner: true,  isWritable: true  }, // admin (Mason)
      { pubkey: ALEX_7ZDJ, isSigner: true,  isWritable: false }, // cosigner (Alex 7ZDJ)
      { pubkey: VAULT,     isSigner: false, isWritable: true  }, // vault_state
    ],
    data,
  });
  const { blockhash, lastValidBlockHeight } = await connection.getLatestBlockhash("confirmed");
  const msg = new TransactionMessage({
    payerKey: ALEX_7ZDJ,           // FEE PAYER = 7ZDJ so Phantom pays
    recentBlockhash: blockhash,
    instructions: [ix],
  }).compileToV0Message();
  const vtx = new VersionedTransaction(msg);
  vtx.sign([MASON]);               // Mason partial-sig; 7ZDJ slot stays empty for Phantom
  return {
    txBase64: Buffer.from(vtx.serialize()).toString("base64"),
    blockhash,
    lastValidBlockHeight,
  };
}

// --- HTTP --------------------------------------------------------------------
function readBody(req) {
  return new Promise((resolve) => {
    let b = "";
    req.on("data", (c) => (b += c));
    req.on("end", () => resolve(b));
  });
}
function json(res, code, obj) {
  const s = JSON.stringify(obj);
  res.writeHead(code, { "Content-Type": "application/json", "Content-Length": Buffer.byteLength(s) });
  res.end(s);
}

const PAGE = fs.readFileSync(path.join(__dirname, "index.html"), "utf8");

const server = http.createServer(async (req, res) => {
  try {
    if (req.method === "GET" && (req.url === "/" || req.url === "/index.html")) {
      res.writeHead(200, { "Content-Type": "text/html; charset=utf-8" });
      res.end(PAGE);
      return;
    }
    if (req.method === "POST" && req.url === "/build") {
      const out = await buildPartialSigned();
      console.log(`[build] fresh tx @ blockhash ${out.blockhash} (validUntil ${out.lastValidBlockHeight})`);
      json(res, 200, out);
      return;
    }
    if (req.method === "POST" && req.url === "/rpc") {
      const body = await readBody(req);
      const upstream = await fetch(HELIUS_URL, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body,
      });
      const text = await upstream.text();
      res.writeHead(upstream.status, { "Content-Type": "application/json" });
      res.end(text);
      return;
    }
    res.writeHead(404, { "Content-Type": "text/plain" });
    res.end("not found");
  } catch (e) {
    console.error("[err]", e.message);
    json(res, 500, { error: e.message });
  }
});

server.listen(PORT, "127.0.0.1", () => {
  console.log("=".repeat(70));
  console.log("turf-vault update_signers cosign server  (DEVNET ONLY)");
  console.log("=".repeat(70));
  console.log("  rotation : [F6f8, 7ZDJ, Cyt]  ->  [8K81, 7ZDJ, Cyt]   (F6f8 evicted)");
  console.log("  admin    : Mason  Cyt…qWjrR   (partial-signed server-side)");
  console.log("  cosigner : Alex   7ZDJ…59Tcr  (signs in Phantom)");
  console.log("  feePayer : Alex   7ZDJ…59Tcr");
  console.log("  program  :", PROGRAM.toBase58());
  console.log("  vault    :", VAULT.toBase58());
  console.log("  RPC      : Helius DEVNET (proxied; URL not exposed to page)");
  console.log("-".repeat(70));
  console.log(`  OPEN:  http://localhost:${PORT}`);
  console.log("=".repeat(70));
});
