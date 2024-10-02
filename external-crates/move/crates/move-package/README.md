---
id: move-package
title: Move Package
custom_edit_url: https://github.com/move-language/move/edit/main/language/tools/move-package/README.md
---

# Summary

The Move package crate contains the logic for parsing, resolving, and
building Move packages. It is meant to be used as a library, both for
building packages (e.g., by the Move CLI), or by other applications that
may be working with Move packages. The package system is split into three
main phases: parsing, resolution, and compilation.

## Parsing and Manifest Layout

The parsing and manifest layout logic is defined in the
[`./src/source_package`](./src/source_package) directory. This defines the
layout and the set of required and optional directories for a Move package
in [`./src/source_package/layout.rs`](./src/source_package/layout.rs), it
defines the format of the parsed Move package manifest ("`Move.toml`") in
[`./src/source_package/parsed_manifest.rs`](./src/source_package/parsed_manifest.rs),
and it defines the parser for the Move package manifest in
[`./src/source_package/manifest_parser.rs`](./src/source_package/manifest_parser.rs).
Note that we don't have a tokenizer/lexer as we use the TOML lexer. This
also resolves git dependencies to where they will live on the local file
system (but does not clone them).

## Resolution

The resolution phase is responsible for resolving all packages and building
the package graph which represents the dependency relations between
packages. It is also responsible for ensuring that all named addresses have
a value assigned to them, and that there are no conflicting assignments.
The package graph is rooted at the package being built and is a DAG.

Discovering the full set of transitive dependencies (including
dev-dependencies), regardless of the current build configuration
results in a `DependencyGraph` which can optionally be serialized into
(and deserialized out of) a `LockFile`. If a `DependencyGraph` can be
created, it is guaranteed to:

- Be acyclic.

- Contain no conflicting dependencies, where the same package name
  is required to come from two distinct sources.

- Have well-nested relative local dependencies, where for all
  dependency chains `R -> L0 -> L1 -> ... -> Ln` with `R` being
  remote, `Li` being local with a relative path, and `X -> Y` meaning
  `X` depends on `Y`, the paths of all `Li`s are sub-directories of
  the repository containing `R` (but not necessarily sub-directories
  of each other).

The logic for exploring transitive dependencies is found in
[`./src/resolution/dependency_graph.rs`](./src/resolution/dependency_graph.rs),
and the logic for lock files (creation, commit, schema) is found in
the [`./src/resolution/lock_file`](./src/resolution/lock_file)
directory.

A `DependencyGraph` is further processed into a `ResolvedGraph` which
is specific to the current build configuration (e.g. only includes
dev-dependencies and dev-address assignments if dev-mode is enabled),
and further includes the following for each package:

- Its source digest, which is a hash of its manifest and source files.

- Its "renaming" table which includes in-scope addresses that
  originate from dependencies but have been renamed.

- Its "resolution table", which is a total mapping from its in-scope
  named addresses to numerical addresses.

If a `ResolvedGraph` can be created, it guarantees that:

- All packages exist at their sources, and all of them are available
  locally (fetched from remote sources such as git if necessary).

- Package source digests match the source digests (if supplied) for
  the dependencies they satisfy.

- All packages have valid renaming tables, where all their bindings
  refer to valid addresses in their dependencies and introduce
  bindings that do not overlap with other renamings for the same
  package.

- A complete named address assignment exists, wherein every named
  address in scope for every package is bound to some numerical
  address.

- A consistent named address assignment exists, wherein if address `A`
  in package `P` is equivalent to address `B` in package `Q`, then `A`
  and `B` are assigned the same numerical address. Informally, two
  named addresses (across packages) are equivalent if they are related
  by scope or renamings.  Formally, for packages `P`, `Q` and named
  addresses `x`, `y`, this equivalence is the transitive reflexive
  closure of the relation that corresponds `x` in `P` to `y` in `Q`
  when,
  - `P` depends on `Q`,
  - `y` is in scope in `Q`,
  - `P`'s renaming binds `x` to `(Q, y)`, or
  - `x` is not in `P`'s renaming, and `x = y`.

Named address assignment is implemented by unification, so if:

- `P` depends on `Q`, renaming its `QA` to `PA`, and assigns `0x42` to `PA`.
- `Q` depends on `R`, renaming its `RA` to `QA`,
- `R` introduces unbound address `RA`.

This results in a complete and consistent named address assignment where,

- `PA = 0x42` in `P`'s resolution table,
- `QA = 0x42` in `Q`'s resolution table,
- `RA = 0x42` in `R`'s resolution table,

even though only one concrete name was assigned.  Similarly, if:

- `P` depends on `Q`, and assigns `0x42` to `QA`,
- `P` depends on `R`, and assigns `0x43` to `RA`,
- `Q` depends on `S` renaming its `SA` to `QA`,
- `R` depends on `S` renaming its `SA` to `RA`,
- `S` introduces unbound address `SA`.

This results in an inconsistent named address assignment because it
requires `SA` to be bound to both `0x42` and `0x43`.

[`./src/resolution/resolution_graph.rs`](./src/resolution/resolution_graph.rs)
defines `ResolvedGraph` and creating one from a `DependencyGraph`,
with support for named address resolution (unification) in
[`./src/resolution/resolving_table.rs`](./src/resolution/resolving_table.rs)
and for calculating source digests in
[`./src/resolution/digest.rs`](./src/resolution/digest.rs).

## Compilation

The final stage of the package system is compilation. All logic relating to
the final build artifacts, or global environment creation, is contained in
the [`./src/compilation`](./src/compilation) directory. The package layout
for compiled Move packages is defined in
[`./src/compilation/package_layout.rs`](./src/compilation/package_layout.rs).

The [`./src/compilation/build_plan.rs`](./src/compilation/build_plan.rs)
contains the logic for driving the compilation of a package and the
compilation of all of the package's dependencies given a valid resolution
graph. The logic in
[`./src/compilation/compiled_package.rs`](./src/compilation/compiled_package.rs)
contains the definition of the in-memory representation of compiled Move
packages and other data structures and APIs relating to compiled Move
packages, along with the logic for compiling a _single_ Move package
assuming all of its dependencies are already compiled and saved to disk.
This is driven by the logic in
[`./src/compilation/build_plan.rs`](./src/compilation/build_plan.rs). The
compilation process is also responsible for generating documentation, ABIs
and the like, along with determining if a cached version of the
to-be-built package already exists and if so, if the cached version
can be used or if the cached copy is invalid and needs to be recompiled.

One important thing to note here is that depending on the compilation
flags, the caching policy may need to be updated and the `compiler_driver`
function that is passed into the compilation process may change. However,
what this function should be is determined by the client of the Move
package library. In particular, when testing even if we are recompiling
with the same flags we cannot cache the root package as we need to compile
it to generate the test plan that will be used by the unit test runner
later on. This gathering of the test plan is inserted into the compilation
process via the `compiler_driver` function that is passed in by the client.
In this case, the [`../move-cli/src/package`](../move-cli/src/package) is
the client and it is responsible for supplying the correct function as the
compiler driver to collect the test plan and to later pass that to the unit
test runner.
