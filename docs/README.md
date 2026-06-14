# TurfVault Docs

Use this index before following any operational instruction in this directory.

## Live References

| Need | File |
|------|------|
| Current program IDs, signer set, upgrade authority, IDL hash | [`CURRENT_DEPLOYMENT.md`](CURRENT_DEPLOYMENT.md) |
| Mainnet key rotation and recovery plan | [`KEY_ROTATION.md`](KEY_ROTATION.md) |

`CURRENT_DEPLOYMENT.md` is the source of truth for live program identity. Do not infer live devnet/mainnet facts from historical specs, audits, generated reports, or old Claude context.

## Historical References

| File | Status |
|------|--------|
| [`MAINNET_LAUNCH.md`](MAINNET_LAUNCH.md) | Historical first-deploy runbook. Do not use as live deployment identity. |
| [`v0.16-spec.md`](v0.16-spec.md) | Historical architecture/spec baseline for the self-custody rewrite. |
| [`SECURITY_AUDIT_2026_05_31.md`](SECURITY_AUDIT_2026_05_31.md) | Historical companion audit. Some findings are fixed in current code; verify against source and `CURRENT_DEPLOYMENT.md`. |
| [`turf-vault-deploy-cost.html`](turf-vault-deploy-cost.html) | Historical generated deployment-cost map. Use for cost intuition, not live instruction names or current authority facts. |

## Agent Rules

- Keep new live deployment facts in `CURRENT_DEPLOYMENT.md`.
- Keep cross-repo setup, credentials, ports, and agent workflows in `mcritchie-studio/docs/agents/`.
- Add date/status banners to historical docs that could otherwise be mistaken for current runbooks.
