# Current Deployment

This is the live operator reference for TurfVault deployment identity. Keep historical launch and rotation plans separate from this file.

## Devnet

| Field | Value |
|-------|-------|
| Program ID | `EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ` |
| Version | v0.25.0 |
| Deployed | 2026-06-11, slot `468716417` |
| Upgrade authority | Squads V4 vault PDA `BW13kgfiG2koFn3WRkte21NW9TFygsD1ge2fNJdjH6kC` |
| Threshold | 2-of-3 for treasury and governance ops |
| Alex Bot signer | `8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd` |
| Alex signer | `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr` |
| Mason signer | `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR` |

The old devnet program `Dx8uGU5w7B9NytDSsW4kseGZuqdVVRq1KY1mGXN2GaCT` is orphaned. Do not use it for live verification.

The retired Alex Bot signer `F6f8...KzhZ` has zero devnet authority after the 2026-06-06 rotation. Agent key material should be referenced through 1Password item names, not pasted into docs.

## Mainnet

| Field | Value |
|-------|-------|
| Program ID | `DaFv83yokwTz8msP9CzJ13eazSGk15NuUTxjkfzJzxMM` |
| Version | v0.24.0 |
| Deployed | 2026-06-08 |
| Expected IDL hash | `5265cc497862ea39d5f3b99bf1fbf42d0cbd51678ca2237b0cc584ee117dde80` |
| Consumer app | `turf-monster-mainnet` |

Mainnet remains on v0.24 until the next upgrade window.

Do not infer the live mainnet version from Turf Monster's committed IDL file alone. The source tree can contain a next-upgrade IDL before the Heroku app has accepted it. Live truth is this deployment note plus the `EXPECTED_IDL_HASH` configured on `turf-monster-mainnet`.

## Upgrade Rule

`anchor deploy` is not the upgrade path for an existing deployed program under Squads authority. Build the program, write a buffer, set the buffer authority to the Squads vault PDA, then execute the upgrade through `scripts/squad-upgrade.js`.

After any upgrade, re-pin Turf Monster from the built IDL, not from `anchor idl fetch`; Squads upgrades do not update the on-chain IDL account.
