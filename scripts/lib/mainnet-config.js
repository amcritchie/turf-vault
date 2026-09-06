/**
 * mainnet-config — resolve the mainnet Squads config out of scripts/squad.json,
 * in EITHER of the two shapes this repo has shipped.
 *
 * WHY THIS EXISTS (2026-09-06, measured — the script aborted on the very file
 * the repo ships). `scripts/initialize-mainnet.js` opened with
 *
 *     if (!cfg.mainnet) abort(`squad.json has no 'mainnet' block — see runbook §3.`);
 *     const m = cfg.mainnet;
 *
 * and the checked-in `scripts/squad.json` has NO `mainnet` key. Its top level
 * IS the mainnet config: `network`, `programId`, `multisigPda`, `vaultPda`,
 * `threshold`, `members`. So `node scripts/initialize-mainnet.js` died at line
 * one of its own guard, every time, on the config the repo committed.
 *
 * That is not dead code. docs/KEY_ROTATION.md §5 points an operator at that
 * script mid key-rotation — the single worst moment to be debugging a
 * config-shape mismatch — and docs/KEY_ROTATION.md §"FIVE changes" item 5
 * recorded the defect explicitly so nobody would rediscover it under pressure.
 *
 * WHY THE FIX IS HERE AND NOT IN squad.json. The alternative repair was to
 * restore a nested `mainnet` block to the config. It was rejected: squad.json's
 * own `_comment` states that `squad-upgrade.js` "reads ONLY these TOP-LEVEL
 * fields (network/programId/multisigPda/vaultPda)" and that "the top-level IS
 * the live mainnet-beta config" — and that is true, `squad-upgrade.js:82-84`
 * reads `cfg.programId` / `cfg.multisigPda` / `cfg.vaultPda`. Adding a nested
 * block would falsify that sentence AND put two copies of the same four
 * addresses in one file, read by two different scripts. The failure mode of
 * that duplication is upgrading one program while initializing another. One
 * source of truth wins; the script learns the shape instead.
 *
 * The NESTED shape is still accepted, because it is what the older runbook
 * prose describes and what a rotation branch may still carry. Nested wins when
 * both are present: an explicit block is a deliberate statement, the top level
 * is an inference.
 *
 * THE FLAT PATH REQUIRES AN EXPLICIT `network`, DELIBERATELY. `initialize` is
 * a one-shot, irreversible write, and this script hardcodes MAINNET Circle
 * USDC / Tether USDT mints. A flat config is only a mainnet config if it says
 * so. A flat `"network": "devnet"` config — which the devnet branches carry —
 * would otherwise sail straight through the old guard's replacement and
 * initialize the devnet vault with mainnet mints. That trade (one silent wrong
 * path swapped for another) is exactly what this module refuses to make: an
 * unlabelled or non-mainnet flat config ABORTS.
 */

"use strict";

const PLACEHOLDER_PUBKEY = "11111111111111111111111111111111";
const MAINNET_CLUSTER = "mainnet-beta";

// The three addresses that make a block a Squads deployment config. These are
// the same fields squad-upgrade.js reads, deliberately — if a block cannot
// drive an upgrade it cannot drive an initialize either.
const REQUIRED_ADDRESS_FIELDS = ["programId", "multisigPda", "vaultPda"];
const REQUIRED_MEMBER_ROLES = ["alex_bot", "alex", "mason"];
const DEFAULT_THRESHOLD = 2;

class MainnetConfigError extends Error {
  constructor(message) {
    super(message);
    this.name = "MainnetConfigError";
  }
}

/**
 * How a field is spelled in an operator-facing message. The two shapes name
 * the same field differently, and an error that says `mainnet.programId` when
 * the file has no `mainnet` key sends the operator hunting for a key that is
 * not there — which is the whole class of failure this module exists to end.
 */
function fieldRef(shape, field) {
  return shape === "nested"
    ? `squad.json mainnet.${field}`
    : `squad.json's top-level ${field}`;
}

function isObject(value) {
  return !!value && typeof value === "object" && !Array.isArray(value);
}

function hasAddressFields(block) {
  return (
    isObject(block) &&
    REQUIRED_ADDRESS_FIELDS.every(
      (k) => typeof block[k] === "string" && block[k].length > 0
    )
  );
}

function missingAddressFields(block) {
  if (!isObject(block)) return REQUIRED_ADDRESS_FIELDS.slice();
  return REQUIRED_ADDRESS_FIELDS.filter(
    (k) => typeof block[k] !== "string" || block[k].length === 0
  );
}

/**
 * Pick the mainnet block out of a parsed squad.json.
 *
 * @returns {{ shape: "nested"|"flat", label: string, block: object }}
 * @throws  {MainnetConfigError} when the config is neither shape, or is a flat
 *          config that does not declare itself mainnet-beta.
 */
function selectMainnetBlock(cfg) {
  if (!isObject(cfg)) {
    throw new MainnetConfigError(
      `squad.json did not parse to a JSON object (got ${
        Array.isArray(cfg) ? "an array" : typeof cfg
      }).`
    );
  }

  // NESTED first: an explicit `mainnet` block is a deliberate statement and
  // outranks anything inferred from the top level.
  if ("mainnet" in cfg) {
    const missing = missingAddressFields(cfg.mainnet);
    if (missing.length > 0) {
      throw new MainnetConfigError(
        `squad.json has a 'mainnet' block, but it is missing ${missing
          .map((k) => `mainnet.${k}`)
          .join(", ")}. ` +
          `Fill it in (runbook §3), or delete the block and use the flat top-level shape.`
      );
    }
    return { shape: "nested", label: "mainnet", block: cfg.mainnet };
  }

  // FLAT: the top level IS the config — the shape squad.json ships and
  // squad-upgrade.js reads.
  if (hasAddressFields(cfg)) {
    if (typeof cfg.network !== "string" || cfg.network.length === 0) {
      throw new MainnetConfigError(
        `squad.json's top level carries ${REQUIRED_ADDRESS_FIELDS.join(
          "/"
        )} but no "network" key, so this ` +
          `script cannot tell which cluster it describes. Add "network": "${MAINNET_CLUSTER}" (or nest the ` +
          `block under 'mainnet'). Refusing to guess on an irreversible mainnet write.`
      );
    }
    if (cfg.network !== MAINNET_CLUSTER) {
      throw new MainnetConfigError(
        `squad.json's top level is a flat config for network "${cfg.network}", not "${MAINNET_CLUSTER}". ` +
          `initialize-mainnet.js hardcodes the mainnet Circle USDC and Tether USDT mints and would ` +
          `initialize the wrong cluster's vault with the wrong currencies. Refusing.`
      );
    }
    return { shape: "flat", label: "top level", block: cfg };
  }

  const keys = Object.keys(cfg);
  throw new MainnetConfigError(
    `squad.json carries neither mainnet config shape this script understands.\n` +
      `  · NESTED — a 'mainnet' block holding ${REQUIRED_ADDRESS_FIELDS.join(
        " / "
      )}.\n` +
      `  · FLAT   — those same fields at the TOP LEVEL, plus "network": "${MAINNET_CLUSTER}".\n` +
      `  top-level keys found: ${keys.length ? keys.join(", ") : "(none)"}\n` +
      `See RUNBOOK §3 and docs/KEY_ROTATION.md §3.`
  );
}

/**
 * Resolve AND validate the mainnet config: shape, placeholders, member set.
 *
 * @returns {{ shape, label, network, programId, multisigPda, vaultPda,
 *             members: {alex_bot, alex, mason}, threshold: number }}
 * @throws  {MainnetConfigError}
 */
function loadMainnetConfig(cfg) {
  const { shape, label, block } = selectMainnetBlock(cfg);

  // Refuse placeholder values (runbook §3 replaces these).
  for (const k of REQUIRED_ADDRESS_FIELDS) {
    if (block[k] === PLACEHOLDER_PUBKEY) {
      throw new MainnetConfigError(
        `${fieldRef(
          shape,
          k
        )} is still the placeholder '${PLACEHOLDER_PUBKEY}'. Run runbook §3 first.`
      );
    }
  }

  for (const role of REQUIRED_MEMBER_ROLES) {
    const member = block.members?.[role];
    if (typeof member !== "string" || member.length === 0) {
      throw new MainnetConfigError(
        `${fieldRef(shape, `members.${role}`)} is missing.`
      );
    }
    if (member === PLACEHOLDER_PUBKEY) {
      throw new MainnetConfigError(
        `${fieldRef(
          shape,
          `members.${role}`
        )} is still the placeholder '${PLACEHOLDER_PUBKEY}'. Run runbook §3 first.`
      );
    }
  }

  return {
    shape,
    label,
    network:
      typeof block.network === "string" ? block.network : MAINNET_CLUSTER,
    programId: block.programId,
    multisigPda: block.multisigPda,
    vaultPda: block.vaultPda,
    members: {
      alex_bot: block.members.alex_bot,
      alex: block.members.alex,
      mason: block.members.mason,
    },
    threshold: block.threshold ?? DEFAULT_THRESHOLD,
  };
}

module.exports = {
  DEFAULT_THRESHOLD,
  MAINNET_CLUSTER,
  MainnetConfigError,
  PLACEHOLDER_PUBKEY,
  fieldRef,
  REQUIRED_ADDRESS_FIELDS,
  REQUIRED_MEMBER_ROLES,
  loadMainnetConfig,
  selectMainnetBlock,
};
