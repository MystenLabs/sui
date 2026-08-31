# On-Chain Package Fetching

## Overview

On-chain dependencies allow Move packages to depend on other packages by their on-chain
address, without requiring source code. This is needed because MVR is currently
source-only, which breaks when source isn't published or doesn't match on-chain bytecode.

A user declares an on-chain dependency in their `Move.toml`:
```toml
[dependencies]
my_dep = { on-chain = true }

[dep-replacements.mainnet]
my_dep = { on-chain = "0x1234..." }
```

The package system fetches the bytecode and linkage table from the network (via the
`MoveFlavor` trait), generates a synthetic `Move.toml` and `Published.toml` in a local
cache, and loads the package like any other dependency. Stub Move source files are
generated from the bytecode so the compiler can type-check against the dependency.

## User-facing manifest format

On-chain dependencies use two forms of the `on-chain` key:

- **`on-chain = true`** in `[dependencies]` — declares that a dependency is on-chain,
  without specifying an address. The address must be provided per-environment in
  `[dep-replacements]`.
- **`on-chain = "0x..."`** in `[dep-replacements]` — provides the on-chain address for a
  specific environment.

```toml
[package]
name = "my_package"

[dependencies]
my_dep = { on-chain = true }

[dep-replacements.mainnet]
my_dep = { on-chain = "0x1234..." }

[dep-replacements.testnet]
my_dep = { on-chain = "0x5678..." }
```

Validation rules enforced during combining:
- `on-chain = "0x..."` in `[dependencies]` is rejected (addresses belong in dep-replacements)
- `on-chain = true` in `[dep-replacements]` is rejected (replacements must have an address)
- `on-chain = true` with no dep-replacement is rejected (no address to fetch)
- `use-environment` on on-chain deps is rejected (on-chain packages use a fixed environment)

## Cache layout

On-chain packages are cached at `~/.move/on-chain/<chain_id>/<address>/`. The cache is
populated on first fetch and reused on subsequent builds. A filesystem lock
(`PackageSystemLock::new_for_onchain`) serializes concurrent access.

```
~/.move/on-chain/<chain_id>/<address>/
├── Move.toml          # generated manifest
├── Published.toml     # generated publication metadata
├── bytecode/           
│   ├── module_a.mv    # serialized CompiledModule bytecode
│   └── module_b.mv
└── sources/            # (future) generated stub Move source
    ├── module_a.move
    └── module_b.move
```

The `bytecode/` directory uses a different path from the compiler's
`build/<pkg>/bytecode_modules/` to avoid clobbering the original bytecode if the package
is recompiled.

The cache check uses manifest existence: if `Move.toml` exists, the fetch is skipped
entirely. The cache is written atomically — bytecode, source stubs, manifest, and
Published.toml are all written in a single locked section, with the manifest written last.

## Generated manifest

The generated `Move.toml` for an on-chain package looks like:

```toml
[package]
name = "_onchain_package"
implicit-dependencies = false

[environments]
_on_chain = "<chain_id>"

[dep-replacements._on_chain]
onchain_0x0002 = { on-chain = "0x0002", override = true }
onchain_0x0003 = { on-chain = "0x0033", override = true }
```

Key design choices:

- **`name = "_onchain_package"`** — a fixed name for all generated packages. Users never need to know
  this name because the rename-from check is skipped for on-chain dependencies.
- **`implicit-dependencies = false`** — prevents the generated manifest from pulling in the
  flavor's implicit dependencies (e.g. `sui`, `std`), which would conflict with the system
  deps already in the graph.
- **`override = true`** — on-chain dep-replacements are overrides so they take precedence
  when the same dependency appears in the graph.
- **Dependency names** — linkage table entries are named `onchain_<original_id>`. These
  names are arbitrary identifiers; the `on-chain` address is what matters.
- **`Published.toml`** — records the package's `published-at`, `original-id`, `version`,
  and `chain-id` under the `_on_chain` environment, following the same schema as
  user-authored Published.toml files.

## Fetching pipeline

When `fetch()` encounters a `Pinned::OnChain { address }`, it delegates to
`on_chain::fetch::fetch_onchain`, which:

1. Computes the cache directory from `MOVE_HOME`, chain ID, and address
2. Acquires an exclusive filesystem lock for the cache directory
3. Checks if the manifest already exists (cache hit → return immediately)
4. Calls `config.flavor.fetch_onchain_package(address)` to download
   `OnChainPackageData` from the network (modules, linkage table, original ID, version)
5. Writes bytecode to `bytecode/<name>.mv`
6. Generates stub source files (currently a no-op, DVX-2119)
7. Generates `Move.toml` via `ParsedManifest` + `RenderToml`
8. Generates `Published.toml` via `ParsedPublishedFile` + `RenderToml`
9. Returns the cache directory path as a `PackagePath`

The `MoveFlavor::fetch_onchain_package` trait method is the only network-touching
operation. `Vanilla` looks up from a pre-populated `BTreeMap` (for testing). `SuiFlavor`
calls `get_object` via gRPC, extracts the `MovePackage` from the object, and maps
`module_map`, `linkage_table`, `original_package_id()`, and `version` to
`OnChainPackageData`.

After fetching, `Package::load` reads the generated manifest and proceeds through the
normal loading pipeline — combining deps, checking environments, loading publications.
The rename-from check is skipped for on-chain deps (the `"_onchain_package"` name won't match
the user's dependency name).

## Environment handling

Environment names are per-package: different packages in the dependency graph may use
different names for the same network. For example, one package might call mainnet
`"mainnet"` while another calls it `"production"`.

On-chain packages are tied to a specific chain (determined by the chain ID at fetch time),
so they don't need environment flexibility. Generated manifests use a single fixed
environment name `_on_chain` mapped to the chain ID.

To connect the caller's environment to the generated package's `_on_chain` environment,
`use_environment` is forced to `_on_chain` during combining for all `OnChainAt`
dependencies (in `CombinedDependency::from_replacement` and
`from_default_with_replacement`). This means the graph builder loads on-chain packages
with the `_on_chain` environment regardless of what the parent package's environment is
called. Explicit `use-environment` on on-chain deps is rejected with an error.

This design means the cache is written once per chain ID + address pair and never needs to
be updated for different environment names.

## Known limitations and future work

- **System package deduplication (DVX-2126):** The linkage table entries are all turned
  into on-chain dep-replacements, including entries for system packages (e.g. `0x1`, `0x2`
  on Sui). System packages are mutable and should instead be resolved as system deps. More
  generally, a package may be reachable both as an on-chain dep (from a linkage table) and
  as a source dep (from the user's manifest or system deps). These need to be
  deduplicated, likely in a post-processing step after the full graph is built. This blocks
  practical end-to-end testing since almost all on-chain packages depend on system
  packages.
- **Stub source generation (DVX-2119):** `write_source` is currently a no-op. Stub Move
  source files need to be generated from bytecode so the compiler can type-check against
  on-chain dependencies. A draft implementation exists in PR #26555
  (`mdgeorge/onchain-stubs`) and will be integrated into `on_chain/move_source.rs`.
- **Manifest serialization mismatch (DVX-2125):** `ManifestDependencyInfo`'s derived
  `Serialize` doesn't match its custom `Deserialize`. The `RenderToml` impl on
  `ParsedManifest` works around this by rendering dep-replacements manually. The proper fix
  is to decouple the digest computation from serialization, then fix the serializer.
- **`move_home` threading (DVX-2127):** The on-chain cache, git cache, and lock files all
  hardcode `MOVE_HOME`. Threading a configurable `move_home` through `PackageConfig` would
  enable per-test cache isolation. Currently, tests use unique addresses to avoid
  collisions.
- **Test-publish (DVX-2120):** Support for `sui move test-publish` with on-chain
  dependencies.
- **Unit tests (DVX-2060):** Support for `sui move test` with on-chain dependencies.
