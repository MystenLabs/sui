# Sui Documentation Audit Report

**Date:** 2026-05-31  
**Auditor:** Capy (automated audit)  
**Scope:** `~/code/sui/docs/content/` (530 files) against codebase at `680ca90782c09c79bb6a2f94e6157de1a4f6a13e` (`680ca90782`) on current `main`  
**Method:** Cross-referenced docs against current source, config schema, CLI clap surfaces, docs console snippets, gRPC service registrations, and recent docs/code changes since the previous audit on `capy/docs-audit-report`.

---

## Changelog Since Previous Audit (2026-05-24)

Previous audit: **9 issues**.  
Current audit: **8 issues**.

> Note: the 2026-05-24 report counted a `fire-drill` CLI docs gap. Per Jessie’s 2026-05-27 instruction, this audit intentionally excludes `fire-drill` from the report and Slack summaries unless that guidance is reversed.

### Fixed since last audit

1. **`completion` now has a dedicated CLI docs page.** The gap called out last week is resolved by `docs/content/references/cli/completion.mdx`.
2. **The custom indexer guide now recommends durable production inputs.** `docs/content/develop/accessing-data/custom-indexer/build.mdx` now leads with gRPC streaming plus the GCS-backed checkpoint bucket, instead of treating the 30-day HTTPS endpoint as the canonical production command.

### Still open from last audit

1. **Full node docs still point readers at the YAML template for the “complete list” of `rpc` options even though that template has no `rpc:` block.**
2. **The main `sui client` reference is still incomplete for several shipped commands.**
3. **The zkLogin demo still defaults to Devnet while the main guide now defaults to Testnet.**
4. **DeepBook Predict still carries stale freshness/versioning markers.**
5. **The new v2alpha ledger-history APIs and their required full-node knobs remain effectively undocumented in the public gRPC/operator docs.**
6. **Canonical GitHub URL casing is still inconsistent in several docs pages.**

### New / newly surfaced issues in this audit

1. **The authenticated-events guide documents RPC names that no longer exist in code.**
2. **The TypeScript gRPC client scaffold mixes `sui/node/v2` and `sui/rpc/v2` paths in the same example.**

---

## Summary

Found **8 issues** across operator docs, CLI docs, gRPC API docs, app-integration guides, and content-quality checks.

- **High:** 0
- **Medium:** 6
- **Low:** 2

No high-severity security or operator-safety issues surfaced in this pass. The remaining drift is concentrated in API/config discoverability, outdated integration guidance, and a few copy/paste docs bugs.

---

## MEDIUM Severity

### 1. Full node docs still point to the YAML template for the “complete list” of `rpc` options, but the template still has no `rpc:` block
- **Category:** Config / operator docs
- **Files:**
  - `docs/content/operators/full-node/sui-full-node.mdx` (lines 99-106)
  - `crates/sui-config/data/fullnode-template.yaml` (lines 1-32)
  - `crates/sui-config/src/rpc_config.rs` (lines 9-79, 109-125)
- **What’s wrong:** The docs say “Refer to the full node YAML template for the complete list of available `rpc` options.” The template still only exposes legacy top-level settings like `json-rpc-address`; it does not contain any `rpc:` block. The actual `RpcConfig` already exposes more settings than the docs list, including `max-json-move-value-response-size`, `index-initialization`, `ledger-history-indexing`, `ledger-history`, and `display`.
- **What it should say:** Either add a real `rpc:` block to the template or point readers to generated/schema-backed config docs instead of calling the template exhaustive.
- **Impact:** Operators looking for supported RPC knobs will still miss real runtime controls and may assume the template is authoritative when it is not.

### 2. `client.mdx` still omits examples for several shipped `sui client` commands
- **Category:** CLI commands & flags
- **Files:**
  - `docs/content/references/cli/client.mdx` (lines 23-25, 171-220)
  - `docs/content/snippets/console-output/sui-client-help.mdx` (lines 17, 33, 39, 52-55)
  - `crates/sui/src/client_commands.rs` (lines 221-225, 315-417, 472-494)
- **What’s wrong:** The help snippet still exposes `execute-combined-signed-tx`, `party-transfer`, `send-funds`, `serialized-tx`, and `serialized-tx-kind`, but the main `client.mdx` examples page still has no examples or link-outs for most of them. `execute-combined-signed-tx` is at least covered in the offline-signing guide, but `party-transfer`, `send-funds`, `serialized-tx`, and `serialized-tx-kind` still have no real docs coverage outside the help snippet.
- **What it should say:** Add at least short reference examples for these shipped commands, with links to deeper workflow guides where they already exist.
- **Impact:** Users still encounter supported commands in `sui client --help` that the main CLI reference does not explain.

### 3. zkLogin demo still defaults to Devnet even though the main zkLogin guide now defaults to Testnet
- **Category:** Code examples
- **Files:**
  - `docs/content/sui-stack/zklogin-integration/zklogin-demo.mdx` (lines 168-176)
  - `docs/content/sui-stack/zklogin-integration/index.mdx` (lines 44-45)
- **What’s wrong:** The demo `.env` still ships with `VITE_NETWORK=devnet` and `VITE_SUI_GRPC_URL=https://fullnode.devnet.sui.io:443`, while the main guide now uses Testnet as the default public-network example.
- **What it should say:** Default the demo to Testnet, with Devnet called out only as an opt-in choice for cutting-edge testing.
- **Impact:** Builders following the demo still start on the most reset-prone public network unless they notice the inconsistency.

### 4. DeepBook Predict docs still advertise a stale verification date while remaining pinned to a dated Testnet branch
- **Category:** Documentation freshness / coordination-required content
- **Files:**
  - `docs/content/onchain-finance/deepbook-predict/deepbook-predict.mdx` (lines 16-26)
  - `docs/content/onchain-finance/deepbook-predict/contract-information.mdx` (lines 17-18, 29-36, 120-124)
- **What’s wrong:** The docs explicitly say they are pinned to `predict-testnet-4-16`, but the page frontmatter still claims `last_verified: 2025-04-16`. More than a year later, the verification marker is stale enough to undermine trust instead of increasing it.
- **What it should say:** Re-verify the package IDs / public server / source branch with the DeepBook Predict owners and refresh the verification date, or move the content under a more obviously temporary preview label.
- **Impact:** Readers get a stale “verified” signal on content that is already tied to a dated Testnet-only branch and likely needs owner confirmation.

### 5. The public gRPC docs still do not document the v2alpha ledger-history APIs or the full-node knobs required to support them
- **Category:** API / operator docs drift
- **Files:**
  - `crates/sui-rpc-api/src/grpc/v2alpha/ledger_service/mod.rs` (lines 4-10, 27-62)
  - `crates/sui-rpc-api/src/lib.rs` (lines 179-185, 224-232, 249-251)
  - `crates/sui-config/src/rpc_config.rs` (lines 58-79, 109-125)
  - `docs/content/develop/accessing-data/grpc/using-grpc.mdx` (lines 117-159)
  - `docs/content/develop/accessing-data/grpc/what-is-grpc.mdx` (lines 40-50)
- **What’s wrong:** Code now ships and registers `sui.rpc.v2alpha.LedgerService` list endpoints (`list_checkpoints`, `list_transactions`, `list_events`) plus the `ledger-history-indexing` and `ledger-history.*` config surface. The public gRPC docs still only enumerate the v2 services and only show `sui.rpc.v2.*` examples.
- **What it should say:** Add a dedicated docs section (or a clear expansion of the existing gRPC guide) covering the v2alpha list APIs, when to use them, retention expectations, and the `rpc` settings operators must enable for historical indexes.
- **Impact:** Operators and client developers still cannot discover or correctly enable a shipped API surface from the docs alone.

### 6. The authenticated-events guide still documents removed RPC names instead of the current v2alpha APIs
- **Category:** API docs drift
- **Files:**
  - `docs/content/develop/accessing-data/authenticated-events.mdx` (lines 127-140, 158-202)
  - `crates/sui-light-client/src/authenticated_events/mod.rs` (lines 21-25, 217-247, 525-532)
  - `crates/sui-rpc-api/src/grpc/v2alpha/proof_service/mod.rs` (lines 4-22)
  - `crates/sui-rpc-api/src/grpc/v2alpha/ledger_service/mod.rs` (lines 4-10, 27-62)
- **What’s wrong:** The guide still tells readers to use `EventService.ListAuthenticatedEvents` and `ProofService.GetObjectInclusionProof`. Current code uses `sui.rpc.v2alpha.LedgerService.ListEvents` for the event stream and `ProofService.GetCheckpointObjectProof` for proofs; there is no in-tree `EventService` or `GetObjectInclusionProof` implementation anymore.
- **What it should say:** Rewrite the API section to match the current light-client/code path: `v2alpha::LedgerService.ListEvents` plus `v2alpha::ProofService.GetCheckpointObjectProof`, including any read-mask or pagination caveats that the current client depends on.
- **Impact:** Builders following this page will try to call RPC methods that no longer match the shipped implementation.

---

## LOW Severity

### 7. The TypeScript gRPC scaffold in `using-grpc.mdx` mixes `sui/node/v2` and `sui/rpc/v2` paths in the same example
- **Category:** Copy/paste docs bug
- **Files:**
  - `docs/content/develop/accessing-data/grpc/using-grpc.mdx` (lines 184-198, 207)
  - `crates/sui-rpc-api/src/lib.rs` (lines 175-185)
  - `crates/sui-light-client/src/authenticated_events/mod.rs` (lines 19-24)
- **What’s wrong:** The example project tree says generated protos live under `protos/sui/node/v2/`, but the same section immediately tells readers to download `sui/rpc/v2` protos and then loads `protos/sui/rpc/v2/ledger_service.proto`. In-tree client/server code also consistently imports `sui::rpc::v2` and `sui::rpc::v2alpha` packages.
- **What it should say:** Normalize the scaffold to `sui/rpc/v2` everywhere.
- **Impact:** Low, but it is a copy/paste footgun for anyone following the TypeScript setup literally.

### 8. `MystenLabs` GitHub URL casing is still inconsistent in several docs pages
- **Category:** Content quality / canonical URLs
- **Files:**
  - `docs/content/onchain-finance/examples-patterns/loyalty-tokens.mdx:33`
  - `docs/content/snippets/quick-install.mdx:12`
  - `docs/content/getting-started/onboarding/sui-install.mdx:62`
  - `docs/content/sui-stack/walrus/sui-stack-walrus-sites.mdx:25,118`
- **What’s wrong:** These pages still use `mystenLabs` or `Mystenlabs` instead of canonical `MystenLabs` in GitHub and raw GitHub URLs.
- **What it should say:** Normalize all GitHub/raw GitHub URLs to canonical `MystenLabs` casing.
- **Impact:** Low. The URLs currently resolve, but the inconsistency is sloppy and easy to avoid.

---

## Notes

### What improved since last audit
- `completion` now has a dedicated CLI reference page.
- The custom indexer guide now leads with gRPC streaming plus GCS-backed fallback for production, instead of pushing the retention-limited HTTPS endpoint as the canonical default.
- The CLI/docs drift around `fire-drill` was intentionally excluded from this audit per the 2026-05-27 instruction.

### Systemic patterns worth addressing
1. **Operator/config docs still drift when config structs gain new fields.** The `rpc` docs are lagging both the template and newly added ledger-history knobs.
2. **New API surfaces are still landing before docs discoverability.** The v2alpha ledger-history endpoints and authenticated-events RPC shape are the clearest examples this week.
3. **The CLI reference still trails the actual clap surface.** Even with PTB-first guidance, shipped commands should not exist only in `--help` output.
4. **Freshness markers need owner workflows behind them.** `last_verified` only helps if somebody actually re-verifies or retires the page.

### Recommended follow-up order
1. Document the full `rpc` surface, including `ledger-history-indexing`, `ledger-history.*`, and the v2alpha list APIs.
2. Fix the authenticated-events page so it matches the current RPC method names and client flow.
3. Fill the remaining `client.mdx` command gaps.
4. Switch the zkLogin demo defaults to Testnet.
5. Re-verify DeepBook Predict content with the owning team.
6. Clean up the TypeScript gRPC scaffold path mismatch and normalize GitHub URL casing.
