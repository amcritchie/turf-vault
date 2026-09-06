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
| Upgrade authority | Squads V4 vault PDA `Bk9sS7iiSRL18vuo2KVzkeGw7EekKqxMCjrdoyGGdJm` |
| Threshold | 2-of-3 for treasury and governance ops |
| Alex Bot signer | `8K81w4e6UcB7TiANhM9N8sAgijJvTxxybRi8AENRaRYd` |
| Alex signer | `7ZDJp7FUHhuceAqcW9CHe81hCiaMTjgWAXfprBM59Tcr` |
| Mason signer | `CytJS23p1zCM2wvUUngiDePtbMB484ebD7bK4nDqWjrR` |
| Expected IDL hash | `5265cc497862ea39d5f3b99bf1fbf42d0cbd51678ca2237b0cc584ee117dde80` |
| Consumer app | `turf-monster-mainnet` |

Mainnet remains on v0.24 until the next upgrade window.

**The upgrade authority is per cluster.** Mainnet's Squads vault PDA (`Bk9sS7ii...`) is a different vault from devnet's (`BW13kgfi...`). Never carry one cluster's vault address to the other; the devnet address is the one that historically leaked into mainnet-facing prose. The `VaultState` signer set and threshold are identical on both clusters today, but nothing in the program ties them together — each cluster's `VaultState` was written by its own `initialize`, and `update_signers` can move one without the other. Read the cluster you mean.

Do not infer the live mainnet version from Turf Monster's committed IDL file alone. The source tree can contain a next-upgrade IDL before the Heroku app has accepted it. Live truth is this deployment note plus the `EXPECTED_IDL_HASH` configured on `turf-monster-mainnet`.

## Verification

Both tables above were re-verified on-chain on 2026-09-05. Re-verify rather than trust:

```bash
# Program upgrade authority (the "Authority" line):
solana program show DaFv83yokwTz8msP9CzJ13eazSGk15NuUTxjkfzJzxMM --url mainnet-beta
solana program show EQGFJAcABtDb6VXtiijTjZ6cE2UqdvhnqJvoharJbpMJ --url devnet

# In-program signer set: VaultState PDA, seeds [b"vault"].
# Signers sit at byte offsets 8/40/72; the threshold byte is at 104.
solana account GBu44HFJjq61WnS9UV1twcSrCC6SkuXHK8RM6tUKsWzV --url mainnet-beta   # mainnet
solana account J7b5g9uS5M2Nog1Ly1UATXTDMtXdpXK3JffRAHXGHkK2 --url devnet         # devnet
```

`VaultState`'s in-program 2-of-3 is a separate mechanism from the Squads vault that holds the program upgrade authority. Both are 2-of-3; they are not the same multisig.

## Upgrade Rule

`anchor deploy` is not the upgrade path for an existing deployed program under Squads authority. Build the program, write a buffer, set the buffer authority to the Squads vault PDA, then execute the upgrade through `scripts/squad-upgrade.js`.

After any upgrade, re-pin Turf Monster from the built IDL, not from `anchor idl fetch`; Squads upgrades do not update the on-chain IDL account.
