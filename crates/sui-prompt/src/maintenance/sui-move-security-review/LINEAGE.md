# Lineage — derivation of the SM-* catalog from `MystenLabs/skills`

The SM-* rules in this catalog were derived (2026) by analyzing the constructive
"how to write correct Sui Move" guidance in the public
[`MystenLabs/skills`](https://github.com/MystenLabs/skills) repository. Each rule is the
**inversion** of a "must / never / always" prescription in the upstream skills — every
constructive rule implies a vulnerability when violated.

## Pinned upstream snapshot

- Repository: `https://github.com/MystenLabs/skills`
- Ref at v1 derivation: `main` @ `764f21a95e709f46c60877a59d6ee6f27d9ed91e`
- Title of HEAD commit: *"Merge pull request #19 from MystenLabs/fix/skill-gaps-from-dapp-builds"*

## Files scanned at derivation time

All `*.md` files under the following skill bundles in the upstream repo:

- `sui-move/` (SKILL.md, move.md, events-coins.md)
- `object-model/` (SKILL.md, ownership.md, transfers.md, dynamic-fields-and-collections.md, patterns.md, display.md)
- `composable-move-functions/SKILL.md`
- `naming-conventions/SKILL.md`
- `move-unit-testing/SKILL.md`
- `modern-move-syntax/SKILL.md`
- `ptbs/` (SKILL.md, fundamentals.md, commands.md, building.md, troubleshooting.md, cli.md)
- `sui-publish/SKILL.md`
- `sui-move-project/SKILL.md`
- `sui-build-test/SKILL.md`
- `frontend-apps/` (SKILL.md, limitations.md, transactions.md, queries.md, setup.md)
- `sui-sdks/` (typescript.md, rust.md)
- `accessing-data/` (SKILL.md, grpc.md, graphql.md, indexers.md, walrus.md, archival.md, use-cases.md)
- `sui-cli/SKILL.md`, `sui-client/SKILL.md`, `sui-install/SKILL.md`
- `sui-overview/ecosystem.md`

## Per-rule attribution

Every SM-* rule's `Source:` line cites the upstream file(s) it was derived from, written
in the form `Source: MystenLabs/skills → <relative-path>`. Rules tagged `[+domain]` come from
established Sui/Move auditing practice and are NOT directly from the upstream skills (notable
examples: SM-A3 capability–resource binding, SM-B4 type-confusion / fake-object injection,
SM-B5 generic-type substitution / unconstrained witness, and the value-reveal nuance in
SM-L2 randomness).

## Refresh protocol

To refresh the SM-* catalog against an updated `MystenLabs/skills` snapshot:

1. **Clone** `MystenLabs/skills` at the new ref:
   ```sh
   git clone https://github.com/MystenLabs/skills.git /tmp/upstream-skills
   ```
2. **Diff** the file set above against the previously-pinned ref:
   ```sh
   git -C /tmp/upstream-skills diff 764f21a95e709f46c60877a59d6ee6f27d9ed91e..HEAD -- <files>
   ```
3. **Re-scan** each changed file with the lens: *"what constructive rules does this state,
   and does any SM-* rule cite it?"* Confirm existing rules still match; identify new
   prescriptions that imply a new SM-* rule.
4. **Update** the affected SM-* reference file(s); bump the pinned ref in this `LINEAGE.md`.
5. **Large changes** (>5 affected rules) — re-run the original 3-agent thorough scan that
   built v1 (the prompt template isn't checked into this repo; reconstruct from the v1
   commit history or contact the catalog author).
6. **Rebuild the Sui CLI** so the embedded skill content reflects the refreshed catalog.

## Why this matters

The audit skill catalog is downstream of the constructive skills. When Sui/Move evolves (new
framework primitives, new patterns, changed semantics), the constructive skills update first;
the audit catalog must follow. Without explicit lineage tracking, the catalog silently goes
stale. This file is the contract that prevents that.
