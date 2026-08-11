---
name: official-sui-skills
description: >
  Pointer to the official Mysten Labs skills for building on Sui — language fundamentals,
  object model, PTBs, SDKs, publishing, upgrades, frontend integration, accessing on-chain
  data. Maintained upstream at github.com/MystenLabs/skills; pinned to the same ref the
  audit catalog derives from (see maintenance/UPSTREAMS.md). Trigger on "build a contract",
  "publish a package", "upgrade a module or package", "use the TypeScript SDK", "write a PTB",
  "set up a Sui client".
---

# Official Sui skills (upstream pointer)

For building, publishing, and upgrading Move contracts on Sui — and the SDK / CLI /
frontend integration around them — refer to the official skills maintained by
Mysten Labs. This bundle is a pointer, not embedded content.

- Repository: <https://github.com/MystenLabs/skills>
- Pinned snapshot (same upstream snapshot the audit catalog tracks):
  <https://github.com/MystenLabs/skills/tree/764f21a95e709f46c60877a59d6ee6f27d9ed91e>

## High-level scope at the pinned ref

- `sui-move/` — Move on Sui: language fundamentals, events, coins
- `object-model/` — ownership, transfers, dynamic fields, display, patterns
- `ptbs/` — Programmable Transaction Blocks (fundamentals, building, troubleshooting, cli)
- `composable-move-functions/`, `naming-conventions/`, `modern-move-syntax/`,
  `move-unit-testing/`, `sui-move-project/`, `sui-build-test/`
- `sui-publish/` — package publishing
- `sui-cli/`, `sui-client/`, `sui-install/` — CLI / client / install
- `frontend-apps/` — TypeScript SDK integration
- `sui-sdks/` — TypeScript and Rust SDKs
- `accessing-data/` — gRPC, GraphQL, indexers, Walrus, archival
- `sui-overview/` — ecosystem framing

## Fetching individual files

Rendered (browser-friendly, HTML):

  <https://github.com/MystenLabs/skills/blob/764f21a95e709f46c60877a59d6ee6f27d9ed91e/{skill}/{file}.md>

Raw (plain markdown, easier for programmatic consumption):

  <https://raw.githubusercontent.com/MystenLabs/skills/764f21a95e709f46c60877a59d6ee6f27d9ed91e/{skill}/{file}.md>

Pick whichever your fetch tool handles best. Both serve the same content at the
same pinned snapshot.
