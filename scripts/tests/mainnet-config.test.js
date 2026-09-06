/**
 * Regression suite for scripts/lib/mainnet-config.js.
 *
 * THE BUG THIS PINS (2026-09-06). scripts/initialize-mainnet.js opened with
 * `if (!cfg.mainnet) abort(...)`, and the committed scripts/squad.json has no
 * `mainnet` key — so the script aborted on the very file the repo ships, every
 * run, before it reached anything else. docs/KEY_ROTATION.md §5 sends an
 * operator to that script mid key-rotation.
 *
 * WHAT MAKES THIS SUITE EVIDENCE RATHER THAN DECORATION. Two properties, and
 * the second is the one a happy-path-only test skips:
 *
 *   1. It drives the REAL, CHECKED-IN scripts/squad.json through the guard —
 *      not a fixture shaped like it. A fixture written from the code under test
 *      certifies the code, not the artifact. `shippedConfig()` reads the file
 *      off disk, and one test asserts the file still has NO `mainnet` key, so
 *      the suite keeps describing the shape that actually broke.
 *
 *   2. It proves the guard still REFUSES. A guard that accepts everything
 *      passes every happy-path assertion above. `REFUSED` below is a table of
 *      configs that must abort — including the two silent-wrong-path cases the
 *      flat fallback could have introduced: a flat DEVNET config, and a flat
 *      config that declares no cluster at all.
 *
 * Pure Node stdlib (node:test + node:assert), no dependency tree — so it runs
 * in CI's `guards` lane, which deliberately installs nothing.
 *
 *   npm run test:scripts
 */

"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const { spawnSync } = require("node:child_process");

const {
  MainnetConfigError,
  PLACEHOLDER_PUBKEY,
  loadMainnetConfig,
  selectMainnetBlock,
} = require("../lib/mainnet-config");

const SCRIPTS_DIR = path.join(__dirname, "..");
const SQUAD_JSON = path.join(SCRIPTS_DIR, "squad.json");
const INIT_SCRIPT = path.join(SCRIPTS_DIR, "initialize-mainnet.js");

// Verified on-chain 2026-09-06 (see the PR body):
//   solana program show DaFv83… --url mainnet-beta -> Authority Bk9sS7…
//   solana account GBu44…      --url mainnet-beta -> owner DaFv83…, threshold 2
const MAINNET_PROGRAM_ID = "DaFv83yokwTz8msP9CzJ13eazSGk15NuUTxjkfzJzxMM";
const MAINNET_SQUADS_VAULT_PDA = "Bk9sS7iiSRL18vuo2KVzkeGw7EekKqxMCjrdoyGGdJm";
const MEMBERS = {
  alex_bot: "8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd",
  alex: "7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr",
  mason: "CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR",
};

/** The artifact the repo actually ships — read from disk, never reconstructed. */
function shippedConfig() {
  return JSON.parse(fs.readFileSync(SQUAD_JSON, "utf8"));
}

/** A complete, valid FLAT mainnet config — the base every REFUSED case breaks. */
function flatFixture(overrides = {}) {
  return Object.assign(
    {
      network: "mainnet-beta",
      programId: MAINNET_PROGRAM_ID,
      multisigPda: "4H3fP3otjMtupk1DQDjKXYY1dWjT6LNM4H4ZWZ1XcKSX",
      vaultPda: MAINNET_SQUADS_VAULT_PDA,
      threshold: 2,
      members: Object.assign({}, MEMBERS),
    },
    overrides
  );
}

// ---------------------------------------------------------------------------
// 1. THE SHIPPED CONFIG — acceptance: the flat config goes through the guard.
// ---------------------------------------------------------------------------

test("the checked-in squad.json still has NO 'mainnet' key", () => {
  // If this ever fails the repo has switched shapes, and the rest of this
  // suite is describing a file that no longer exists in that form. The old
  // guard (`if (!cfg.mainnet) abort`) died precisely here.
  const cfg = shippedConfig();
  assert.equal(
    "mainnet" in cfg,
    false,
    "squad.json grew a 'mainnet' block — re-read scripts/lib/mainnet-config.js's header before changing this test"
  );
});

test("the checked-in squad.json resolves as the FLAT shape", () => {
  const { shape, label } = selectMainnetBlock(shippedConfig());
  assert.equal(shape, "flat");
  assert.equal(label, "top level");
});

test("the checked-in squad.json passes full validation with the live mainnet values", () => {
  const resolved = loadMainnetConfig(shippedConfig());
  assert.equal(resolved.shape, "flat");
  assert.equal(resolved.network, "mainnet-beta");
  assert.equal(resolved.programId, MAINNET_PROGRAM_ID);
  assert.equal(resolved.vaultPda, MAINNET_SQUADS_VAULT_PDA);
  assert.equal(resolved.threshold, 2);
  assert.deepEqual(resolved.members, MEMBERS);
});

test("the shipped squad.json's vaultPda IS the mainnet program's upgrade authority", () => {
  // squad-upgrade.js reads cfg.vaultPda for the same purpose. If initialize
  // ever resolved a different address than the upgrade path, the two scripts
  // would be driving different Squads vaults — the exact failure a second
  // nested copy of these fields would have made possible.
  const resolved = loadMainnetConfig(shippedConfig());
  assert.equal(resolved.vaultPda, MAINNET_SQUADS_VAULT_PDA);
});

// ---------------------------------------------------------------------------
// 2. THE NESTED SHAPE — still accepted; wins when both are present.
// ---------------------------------------------------------------------------

test("a nested 'mainnet' block is still accepted", () => {
  const resolved = loadMainnetConfig({ mainnet: flatFixture() });
  assert.equal(resolved.shape, "nested");
  assert.equal(resolved.label, "mainnet");
  assert.equal(resolved.programId, MAINNET_PROGRAM_ID);
});

test("a nested block outranks the top level when both are present", () => {
  const nestedOnly = "mnzowM2F9dppGVFGrcTAh5351mMqYunX3b2MvdvgS2S";
  const resolved = loadMainnetConfig(
    Object.assign(flatFixture(), {
      mainnet: flatFixture({ programId: nestedOnly }),
    })
  );
  assert.equal(resolved.shape, "nested");
  assert.equal(resolved.programId, nestedOnly);
});

test("a nested block with no network key still resolves as mainnet-beta", () => {
  const nested = flatFixture();
  delete nested.network;
  const resolved = loadMainnetConfig({ mainnet: nested });
  assert.equal(resolved.network, "mainnet-beta");
});

// ---------------------------------------------------------------------------
// 3. THE GUARD STILL BITES — a guard that accepted anything would pass §1.
// ---------------------------------------------------------------------------

const REFUSED = [
  ["an empty object", {}, /neither mainnet config shape/],
  [
    "a comment-only config",
    { _comment: "notes, no addresses" },
    /neither mainnet config shape/,
  ],
  [
    "a flat config missing multisigPda",
    (() => {
      const c = flatFixture();
      delete c.multisigPda;
      return c;
    })(),
    /neither mainnet config shape/,
  ],
  // THE SILENT WRONG PATH the flat fallback could have introduced: this script
  // hardcodes mainnet USDC/USDT, so a devnet config must not sail through.
  [
    "a flat DEVNET config",
    flatFixture({
      network: "devnet",
      programId: "EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ",
      vaultPda: "BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC",
    }),
    /flat config for network "devnet", not "mainnet-beta"/,
  ],
  [
    "a flat config that declares no cluster",
    (() => {
      const c = flatFixture();
      delete c.network;
      return c;
    })(),
    /no "network" key/,
  ],
  [
    "a flat config whose network is empty",
    flatFixture({ network: "" }),
    /no "network" key/,
  ],
  [
    "a flat config still holding the placeholder programId",
    flatFixture({ programId: PLACEHOLDER_PUBKEY }),
    /still the placeholder/,
  ],
  [
    "a flat config still holding the placeholder vaultPda",
    flatFixture({ vaultPda: PLACEHOLDER_PUBKEY }),
    /still the placeholder/,
  ],
  [
    "a flat config missing members.mason",
    (() => {
      const c = flatFixture();
      delete c.members.mason;
      return c;
    })(),
    /members\.mason is missing/,
  ],
  [
    "a flat config whose members.alex is a placeholder",
    flatFixture({
      members: Object.assign({}, MEMBERS, { alex: PLACEHOLDER_PUBKEY }),
    }),
    /members\.alex is still the placeholder/,
  ],
  [
    "a nested block that is incomplete",
    { mainnet: { programId: MAINNET_PROGRAM_ID } },
    /has a 'mainnet' block, but it is missing/,
  ],
  [
    "a nested block that is null",
    { mainnet: null },
    /has a 'mainnet' block, but it is missing/,
  ],
  ["a JSON array", [], /did not parse to a JSON object/],
  ["a JSON string", "mainnet-beta", /did not parse to a JSON object/],
  ["null", null, /did not parse to a JSON object/],
];

for (const [label, cfg, pattern] of REFUSED) {
  test(`refuses ${label}`, () => {
    assert.throws(
      () => loadMainnetConfig(cfg),
      (err) => {
        assert.ok(
          err instanceof MainnetConfigError,
          `expected MainnetConfigError, got ${err && err.name}`
        );
        assert.match(err.message, pattern);
        return true;
      }
    );
  });
}

test("a bad field is reported with the field path of the shape it was found in", () => {
  // An operator told `mainnet.programId` on a file with no `mainnet` key goes
  // hunting for a key that is not there — the exact confusion this task ends.
  assert.throws(
    () => loadMainnetConfig(flatFixture({ programId: PLACEHOLDER_PUBKEY })),
    /squad\.json's top-level programId is still the placeholder/
  );
  assert.throws(
    () =>
      loadMainnetConfig({
        mainnet: flatFixture({ programId: PLACEHOLDER_PUBKEY }),
      }),
    /squad\.json mainnet\.programId is still the placeholder/
  );
});

test("every REFUSED case differs from the accepted fixture by exactly one property", () => {
  // Keeps the table honest: the base fixture must itself be ACCEPTED, or the
  // refusals above prove nothing about the mutation each one makes.
  const resolved = loadMainnetConfig(flatFixture());
  assert.equal(resolved.shape, "flat");
  assert.equal(resolved.threshold, 2);
});

// ---------------------------------------------------------------------------
// 4. THE SCRIPT ITSELF — the guard the operator actually runs.
// ---------------------------------------------------------------------------

test("initialize-mainnet.js sources its guard from lib/mainnet-config, with no second copy", () => {
  const src = fs.readFileSync(INIT_SCRIPT, "utf8");
  assert.match(src, /require\("\.\/lib\/mainnet-config"\)/);
  // A re-introduced independent read is how the two shapes drift apart again.
  const code = src
    .split("\n")
    .filter((line) => !line.trim().startsWith("//"))
    .join("\n");
  assert.equal(
    /\bcfg\.mainnet\b/.test(code),
    false,
    "initialize-mainnet.js reads cfg.mainnet directly again — route it through lib/mainnet-config"
  );
});

test("initialize-mainnet.js gets PAST the config guard on the shipped squad.json", (t) => {
  // END-TO-END: spawn the real script against the real squad.json. It is
  // expected to die LATER, at the admin keypair load, having already resolved
  // and printed the mainnet program ID — which is only reachable if the config
  // guard accepted the flat file. No RPC is issued before that point.
  try {
    require.resolve("@coral-xyz/anchor", { paths: [SCRIPTS_DIR] });
  } catch (_e) {
    t.skip("runtime deps not installed (CI guards lane installs nothing)");
    return;
  }

  const run = spawnSync(process.execPath, [INIT_SCRIPT], {
    encoding: "utf8",
    timeout: 60000,
    env: Object.assign({}, process.env, {
      SOLANA_ADMIN_KEYPAIR: path.join(__dirname, "no-such-keypair.json"),
    }),
  });

  const out = `${run.stdout || ""}`;
  const err = `${run.stderr || ""}`;
  assert.doesNotMatch(err, /no 'mainnet' block/, "the old abort fired again");
  assert.doesNotMatch(err, /neither mainnet config shape/);
  assert.match(out, /\(flat shape, network mainnet-beta\)/);
  assert.match(out, new RegExp(`Program: ${MAINNET_PROGRAM_ID}`));
  assert.match(
    err,
    /ENOENT|no such file/,
    "expected the LATER keypair failure"
  );
  assert.equal(run.status, 1);
});
