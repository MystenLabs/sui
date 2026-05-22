# Tree Shaking Rework

## Overview

Tree shaking runs after a package is compiled but before it is published. The package
system attaches *implicit* dependencies (the system packages) to every package; tree
shaking inspects the compiled bytecode and retains, in the linkage table that gets
published on-chain, only the dependencies that are actually reachable. Unused implicit
dependencies are dropped so they do not appear on-chain.

The current implementation lives in `crates/sui/src/client_commands.rs` and indexes much
of its work by package *name*. Names are not unique, which has caused bugs. This document
is a plan for a rewrite that operates entirely on **addresses** (original IDs and
published IDs); names survive only as labels in error messages.

The work spans three crates:

- `move-package-alt` — small type changes plus one new accessor.
- `sui-move-build` — the `PackageDependencies` type is reshaped.
- `sui` — the algorithm itself, extracted into a new `tree_shake` module.

## Background

**Linkage.** Every on-chain package has a *published ID* (the identity of its current
version) and an *original ID* (the published ID of its first version). Bytecode refers to
other packages by original ID — the version-independent identity. Each on-chain package
carries a flat **linkage table** mapping every transitive dependency's original ID to the
published ID and version to use at runtime. Linkage is resolved top-down: the root
package's linkage table governs the entire call tree, so a root can force a newer version
of a deep transitive dependency than that dependency was built against.

**The on-chain publish check.** When a package is published or upgraded, the chain
validates its linkage table in `MovePackage::build_linkage_table` (`sui-types`). It
enforces exactly two rules:

1. Every dependency referenced by the bytecode appears in the linkage table.
2. For every dependency `d` in the linkage table, every entry of `d`'s own on-chain
   linkage table is *superseded* by the linkage table — present, at a version `≥` `d`'s.

There is **no** rule relating an upgrade's linkage table to the previous version's, so
tree shaking treats publish and upgrade identically and may freely drop entries an
earlier version carried.

**The package system graph.** The package system reconciles the source-level publication
data (version, published ID, original ID) of every source package and produces a
dependency graph from the declared dependencies and that publication data alone — it does
not read on-chain linkage tables. Assuming the source is correct, this graph already
contains every package that could legitimately appear in the linkage table, at versions
that already satisfy the linkage rules.

**The override rule.** This is enforced by `move-package-alt`'s linkage check
(`graph/linkage.rs`) and is load-bearing for the algorithm below. To resolve a version
conflict on a package, an **override dependency** must be declared on *every* path from
the root to each affected package, and an override is always a *direct* dependency edge.
A conflict that spans two of the root's direct subtrees therefore forces an override at
their common ancestor — the root — which makes the conflicted package a *direct*
dependency of the root.

## Goals

1. Operate purely on addresses; names appear only in error messages.
2. Handle the corner cases correctly, with best-effort early diagnostics.
3. Cover the behaviour with tests, preferring shell tests where practical.

## Data structures

`PackageDependencies` (in `sui-move-build`) is reshaped to be address-keyed:

```rust
pub struct PackageDependencies {
    /// Published dependencies, keyed by original ID.
    pub published: BTreeMap<OriginalID, PublishedDep>,
    /// Names of dependencies with no on-chain publication.
    pub unpublished: BTreeSet<Symbol>,
}

pub struct PublishedDep {
    pub published_id: PublishedID,
    /// Resolved version, used for the supersession check.
    pub version: u64,
    /// Whether this is a declared direct dependency of the root package.
    pub is_direct: bool,
    /// Used only for error messages.
    pub name: Symbol,
}
```

- Keying `published` by `OriginalID` makes the retain step a direct address match and
  structurally enforces "one published version per original ID."
- The dead `invalid` and `conflicting` fields are deleted.
- `PackageDependencies::new` keys entries by original ID, **errors on a duplicate
  original ID** instead of letting `BTreeMap::insert` silently drop one, and marks
  `is_direct` using a new `RootPackage` accessor (below).
- `get_dependency_storage_package_ids` and `get_published_dependencies_ids` have
  identical bodies; collapse them into one method returning the published IDs.

`OriginalID` and `PublishedID` (`move-package-alt/src/schema/shared.rs`) gain `Copy` and
`Debug` derives (`PublishedID` also `Hash`), so they can be used as map keys, copied
freely, and logged.

A new `RootPackage` accessor returns the root's declared direct dependencies as flat
address data (e.g. `direct_dependency_original_ids() -> BTreeSet<OriginalID>`),
implemented internally with the private `PackageInfo::direct_deps()`. `direct_deps()`
**stays private** — exposing a recursable `PackageInfo` walk would let callers reason
about transitive structure outside `move-package-alt`, which is the boundary that method
deliberately protects. A flat `BTreeSet<OriginalID>` is inert data and crosses no
boundary.

## Algorithm

Tree shaking lives in a new module, `crates/sui/src/tree_shake.rs`. Five functions:

```rust
/// Original IDs referenced directly by the modules being published.
fn direct_dep_original_ids(
    with_unpublished_deps: bool,
    compiled_package: &CompiledPackage,
) -> BTreeSet<OriginalID>

/// The on-chain linkage table of the package published at `pkg_id`.
async fn onchain_linkage(
    client: &mut Client,
    pkg_id: PublishedID,
) -> anyhow::Result<BTreeMap<OriginalID, UpgradeInfo>>

/// Pure closure + best-effort validation; no RPC. Unit-tested exhaustively.
fn compute_required_linkage(
    seed: &BTreeSet<OriginalID>,
    candidates: &BTreeMap<OriginalID, PublishedDep>,
    direct_linkages: &BTreeMap<OriginalID, BTreeMap<OriginalID, UpgradeInfo>>,
) -> anyhow::Result<BTreeSet<OriginalID>>

/// RPC shell: builds the seed, fetches the direct deps' tables, calls the closure.
async fn required_linkage_oids(
    client: &mut Client,
    compiled_package: &CompiledPackage,
    with_unpublished_deps: bool,
) -> anyhow::Result<BTreeSet<OriginalID>>

/// Filters `compiled_package.dependency_ids.published` to the required set.
pub(crate) async fn pkg_tree_shake(
    mut client: Client,
    with_unpublished_deps: bool,
    compiled_package: &mut CompiledPackage,
) -> anyhow::Result<()>
```

**The seed.** `direct_dep_original_ids` collects the `immediate_dependencies` of the
considered modules (the root modules, or — with `with_unpublished_deps` — all modules at
address `0x0`) and subtracts those modules' own self-addresses:

> `seed = ⋃(immediate-dep addresses) − ⋃(self addresses)`

The single subtraction excludes both the package's internal module-to-module references
and co-published unpublished dependencies at `0x0`, with no magic-address checks.

**The closure.** `required_linkage_oids` builds the seed, fetches `onchain_linkage` for
every `is_direct` candidate (these fetches are independent and may run concurrently),
and hands everything to `compute_required_linkage`, which computes:

> `required` = the closure of `seed` under the on-chain linkage tables of the root's
> direct dependencies — expanding a node only when it is a direct dependency.

```
required = {}
for o in seed: if o not in candidates { bail (check A1) }
worklist = seed
while o = worklist.pop():
    if o already in required: continue
    required.insert(o)
    if direct_linkages has an entry for o:          # o is a direct dependency
        for (y, info) in direct_linkages[o]:
            if y not in candidates: bail (check A2)
            if candidates[y].version < info.upgraded_version: bail (check B)
            worklist.push(y)
```

`pkg_tree_shake` then retains the address-keyed candidates:

```rust
let required = required_linkage_oids(&mut client, compiled_package, w).await?;
compiled_package.dependency_ids.published.retain(|oid, _| required.contains(oid));
```

The published IDs of the surviving entries become the dependency list of the publish or
upgrade transaction.

## Why fetching only the direct dependencies suffices

This is the subtle, load-bearing property, and it depends on an invariant enforced in a
*different crate*. It is documented as the module-level doc comment of `tree_shake.rs`,
and `graph/linkage.rs` carries a back-reference noting the dependency.

The published linkage table must satisfy on-chain rule 2: it must contain the full
linkage table of every package it retains. We compute it by closing the seed over the
on-chain linkage tables of the root's **declared direct** dependencies *only* — never a
transitive dependency's table. This is correct because of the override rule:

- A version conflict confined to one direct dependency `d`'s subtree is resolved by
  overrides within that subtree. `d` was itself published with those overrides, so `d`'s
  on-chain (flat) linkage table already lists its whole subtree at the final resolved
  versions, each with its complete closure.
- A conflict spanning two of the root's direct subtrees cannot be resolved within
  either; the override is forced up to their common ancestor — the root — which makes the
  conflicted package a *direct* dependency of the root, fetched directly.

So every package in the final linkage table is either a direct dependency (fetched
directly) or lies in some direct dependency's subtree at its final version (carried by
that dependency's flat table). The closure therefore needs no transitive fetches and no
fixpoint over the kept set.

The seed is `bytecode-direct` rather than "all direct dependencies" so that unused
direct dependencies — implicit system packages, version-pinning-only deps — are still
shaken out: they enter `required` only if something reachable actually references them.

This assumes each dependency's source is consistent with its on-chain publication; an
inconsistency is caught by the best-effort checks below or, failing that, rejected by the
chain. **If `move-package-alt` ever lets a transitive package be force-upgraded without
an override reaching the root, this closure will silently drop linkage entries** — hence
the cross-referenced comments.

## Best-effort validation

The chain is the real gate; local validation only produces earlier, clearer errors. It
is cheap (no RPC beyond what the closure already does) and need not be exhaustive. Three
checks, all inline in `compute_required_linkage`:

- **A1 — root references a non-dependency.** A seed element is not a candidate. The
  compiler only emits references to declared dependencies, so this is effectively a
  compiler invariant; a terse defensive `bail!` is sufficient.
- **A2 — a dependency's on-chain dependency is absent from our source.** While expanding
  direct dependency `X`'s table, an entry `Y` is not a candidate:
  > `package <X> depends on on-chain package <Y published id>, but the source for <X>
  > does not depend on that package. This likely indicates a mismatch between the source
  > package and the on-chain bytecode for <X>.`
  `Y` is printed as its published ID (`UpgradeInfo::upgraded_id`) — a concrete object the
  user can inspect; it has no name because it is not in our source.
- **B — version downgrade.** `Y` is a candidate but its resolved version is older than
  `X`'s on-chain linkage table requires:
  > `on-chain, package <X> depends on version <N> of <Y>, but the source build resolves
  > <Y> to the older version <M>. This likely indicates a mismatch between the source
  > packages and what is published on-chain.`

`PackageDependencies::new` additionally errors if the package system resolved two
packages to the same original ID:

> `dependency resolution produced two packages with the same original ID <oid>: <name a>
> and <name b>`

`onchain_linkage` keeps the wrong-object-type error (`object at <id> is not a package`)
and restores the RPC error-message extraction the in-progress sketch had dropped. Names
are attached to fetch failures by the caller via `.with_context`, never passed into
`onchain_linkage`.

## Corner cases

- **Dependency published before tree shaking existed.** A retained dependency's on-chain
  linkage table lists an unused implicit dependency. It is a candidate, so it is retained
  — correctly: on-chain rule 2 requires it. Tree shaking cannot drop cruft pinned by a
  retained dependency; this is why the on-chain fetch exists at all.
- **`with_unpublished_deps`.** Considered modules are all `0x0` modules;
  `seed = immediate − self` excludes the co-published packages while keeping their
  references.
- **System / implicit dependencies.** Ordinary published candidates; referenced ones are
  kept, unreferenced ones shaken.
- **Diamonds.** `published` keyed by `OriginalID` permits one version per package; an
  unresolved diamond fails at `PackageDependencies::new`'s duplicate check.
- **Forced / overridden versions.** Covered by the override-rule argument above — the
  conflicted package is always a direct dependency of some package whose flat table
  carries it.
- **Self address.** `seed = immediate − self` is correct whether the root compiles at
  `0x0` or at its original ID.

## Implementation changes

**`move-package-alt`**
- `src/schema/shared.rs`: add `Copy, Debug` to `OriginalID` and `PublishedID`; add `Hash`
  to `PublishedID`.
- `RootPackage`: add a public accessor returning the root's direct dependencies as
  `BTreeSet<OriginalID>`; keep `PackageInfo::direct_deps()` private.
- `src/graph/linkage.rs`: add a one-line note by the override-rule doc that the property
  is relied upon when computing publishable linkage tables.

**`sui-move-build`**
- Reshape `PackageDependencies` / add `PublishedDep` as above; rewrite
  `PackageDependencies::new`; collapse the two duplicate getter methods.

**`sui`**
- New module `crates/sui/src/tree_shake.rs` with the five functions and the module-level
  correctness doc comment.
- `client_commands.rs`: delete `fetch_move_packages`, `trans_deps_original_ids`, the old
  `pkg_tree_shake`, and the in-progress sketch; update the call site.
- `sui_commands.rs`: update the `pkg_tree_shake` import/call site.
- `client_ptb/builder.rs`: update for the reshaped `PackageDependencies`.
- `tests/shell_tests.rs`: fix the stale `// Temporarily disabled` comment on the
  `with_network` directory.

## Testing

- **Unit tests** for the pure `compute_required_linkage`, using synthetic addresses
  (`OriginalID::from(u16)`): basic shake, pre-tree-shake cruft retained, forced/overridden
  version pulling in a new transitive dependency, diamond, A1, A2, B.
- **Shell tests** under `tests/shell_tests/with_network/tree_shaking/`: publish with
  `sui client publish`, observe the linkage table via `sui move summary` (its output
  includes a `linkage` section), and snapshot it. Migrate the ~9 normal-publish tree-shake
  tests from `cli_tests.rs`. New cases: two dependencies with the same display name (the
  name-collision bug), and an override that drags in a new transitive dependency.
- **Rust tests** retained in `cli_tests.rs`: the two upgrade tests that rely on
  `publish_package_without_tree_shaking` to fabricate a pre-tree-shake on-chain package —
  there is no CLI affordance for that, and it is the correct tool at the Rust layer.
- **Back-testing.** Run the new name-collision and forced-version shell tests against
  merged `main`; they should fail there (genuine bugs) and pass on the rewrite. Anything
  that passes on both was pure cleanup. The tests assert correct behaviour and live only
  on the rewrite branch.
