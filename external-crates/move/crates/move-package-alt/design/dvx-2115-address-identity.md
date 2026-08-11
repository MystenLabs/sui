# Address Identity in the Dependency Graph

## Problem

The dependency system distinguishes between "node" concepts (where a package's source
lives) and "edge" concepts (how a parent references a dependency). The `Pinned` enum
represents nodes, and `DependencyContext` represents edges. However, `published_at` and
`original_id` don't fit cleanly into either category, and the current placement leads to
design problems. The items below are not yet resolved.

## Background: What Are Addresses?

Every published Move package has two addresses:

- **`original_id`**: The address the package was first published at. This serves as the
  package's identity in the Move type system — types are identified by `original_id::module::Type`.
- **`published_at`**: The address where the current version of the package lives on-chain.
  After an upgrade, `published_at` changes but `original_id` stays the same.

These addresses can come from several sources:

1. **The package's own files** — for legacy packages, this may be in `Move.toml` or
   `Move.lock`. For modern packages, it is in `Move.published`.
2. **`dep-replacements` in the parent's manifest** — a parent can override the addresses
   for a dependency, e.g. when the dependency's own manifest has incorrect or missing
   address info.

## The Identity Question

Consider this scenario: a git package `foo` is published twice on the same network — once
at `0xA` and once at `0xB`. These are genuinely different on-chain packages that happen to
share the same source code. A downstream project could depend on both (e.g., to bridge
between them).

This is an unusual corner case and something users probably shouldn't do, but it's
important because it exposes a fundamental question about the design model. We could
detect this situation and fail, but the fact that we're tripping on these kinds of issues
suggests we need to think more clearly about the relationship between source identity,
compiled identity, and address identity.

In the current system:
- Both edges point to the same fetched source (same git SHA, same local path on disk)
- But they have different `published_at` / `original_id` via `dep-replacements`
- The compiled bytecode differs because the self-address differs
- The type systems are disjoint — `0xA::module::T` and `0xB::module::T` are unrelated types

### Are these the same node or different nodes?

**From a source perspective**: same node. The files on disk are identical.

**From a compiled/on-chain perspective**: different nodes. They produce different bytecode,
have different type identities, and are independently upgradeable.

## Design Decision

Addresses are part of the **compiled identity** of a package, but not part of the **source
identity**. This means:

1. **Source fetching and caching** should be based on the source location alone (git SHA +
   path, local path, etc.). Two deps with the same source but different addresses should
   share the same fetched files on disk. The current source cache layout does not need to
   change.

2. **Compiled identity** (for ephemeral publication, linkage, and type checking) must
   account for addresses. The same source with different address overrides produces
   different compilation artifacts. While we don't currently cache compiled artifacts, it
   seems plausible that we would do so in the future, and such a cache would need to be
   keyed by (source location + addresses).

## Implications for `Pinned`

If we treat the same source with different addresses as different nodes in the dependency
graph, then addresses become part of the node identity for **all** `Pinned` variants, not
just `OnChain`. This would mean `PinnedLocalDependency`, `PinnedGitDependency`, etc. would
all carry address information.

`unfetched_path` (for source fetching) would ignore addresses, since the source cache is
keyed by source location alone. But ephemeral publication keys, linkage, and any future
compiled artifact cache would use addresses as part of the key.

### Immediate step: `Pinned::OnChain` should carry `published_at`

Since `published_at` determines *what to fetch* for on-chain deps (analogous to the git
SHA for git deps), it belongs in the `Pinned` enum regardless of the broader design:

```rust
pub(crate) enum Pinned {
    Local(PinnedLocalDependency),
    Git(PinnedGitDependency),
    OnChain { published_at: PublishedID },
    Root(PackagePath),
}
```

This lets `unfetched_path` work naturally for all variants without panics or sentinel
values.

### Ephemeral publication keys should include addresses

Currently, ephemeral publication (`Pub.<env>.toml`) entries are keyed by
`EphemeralDependencyInfo`, which wraps `LocalDepInfo` — just the absolute local path to the
source. The `published_at` and `original_id` are stored as data alongside the key, not as
part of the key itself.

This means two deps with the same source but different address overrides would collide on
the same key. If we support the same source at multiple addresses (whether we treat it as
different nodes or not), the ephemeral publication key should incorporate the address
override.

### Linkage should validate address consistency

DESIGN.md (lines 609-623) describes a check for conflicting addresses: "if two
dependencies in the graph have the same original ID but different published addresses, then
there is a conflict." This check is not yet implemented. With the model above, this
remains important — two edges claiming the *same* `original_id` with *different*
`published_at` for what should be the *same* compiled package is a conflict.

## Status

Remaining work items are tracked in Linear under DVX-2115.
