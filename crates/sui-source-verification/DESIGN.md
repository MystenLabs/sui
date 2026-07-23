# Source verification

`sui client verify-source <package-path> [--toolchain-version <version>]` decides whether a Move
source package compiles to the bytecode and linkage of the on-chain package it was published as.

It rebuilds the source with the toolchain the package was published with, and compares the result to
what is on chain:

```
verify_source(source_path, publication, toolchain_override, env, client, ...)
  ├─ resolve the toolchain version   (publication metadata, the legacy Move.lock, or the override)
  ├─ ensure that toolchain's binary  (cached under $MOVE_HOME/binaries, downloaded if absent)
  ├─ rebuild the source with it      (`move build --dump-bytecode-as-base64`, in a subprocess)
  ├─ fetch the on-chain package      (modules, linkage table, original id) at the publication's address
  ├─ check the original id matches   (the package there is the one the source claims to be)
  └─ compare                         (module bytecode, then linkage)
```

The `publication` — the on-chain address, the original id, and the recorded toolchain — is **not** a
command-line argument. The CLI reads it from the package's own publication metadata via
`move_package_alt::read_publication`, the same code path the package system uses to resolve this
package's address when *linking against it*. See [Security](#security).

## Scope

Only the **root** package is verified: its module bytecode and its linkage table. To verify a
dependency, run the command again against that dependency's source.

The verifier performs no compilation itself. Everything about compiling Move — editions, flavors,
dependency resolution, tree-shaking — is delegated to the downloaded toolchain, which is the same
program that produced the on-chain bytecode. What the verifier knows is narrow:

- how to read a toolchain version out of the publish metadata written by any historical release;
- how to invoke a historical binary, whose command-line flags have changed over time;
- how to fetch an on-chain package and compare bytecode and linkage.

## Resolving the toolchain version

In precedence order:

1. `--toolchain-version`, if given. When it differs from the recorded version it is used anyway, with
   a warning, so a package whose recorded toolchain cannot be built can still be verified with an
   adjacent one.
2. The toolchain version in the package's publication metadata (`Published.toml`).
3. The legacy `Move.lock`'s `[move].toolchain-version`, for packages published under the older system
   (roughly v1.23–v1.62), whose publication metadata carries no such field.

If none of these yields a version, verification fails immediately and asks for `--toolchain-version`;
releases before v1.23 record no toolchain version when they publish.

The recorded `edition` and `flavor` are deliberately **not** forced onto the rebuild. They apply only
when the manifest omits them, in which case the original build used the toolchain's own defaults —
and the rebuild runs that same toolchain, so it will use them again.

## Comparing bytecode

The rebuild emits the root package's modules. Depending on the toolchain they carry either `0x0` or
the package's on-chain address as their self-address; the former is rewritten to the latter. The
module sets must then be identical and every module byte-for-byte equal. All mismatches are reported
together rather than failing on the first.

## Comparing linkage

A linkage table maps each dependency's **original id** to the **storage id** of the version being
linked. Two linkages for the same package can legitimately differ in size, because a build may or may
not have tree-shaken unused dependencies out. So the comparison is an intersection:

1. Map every storage id in the rebuilt linkage to its original id (by fetching the packages).
2. Intersect the two sets of original ids.
3. For every original id in the intersection, require the storage ids to be equal.

An original id present in only one linkage is reported as a warning rather than failing verification.
If the root bytecode matches and every shared dependency resolves to the same version, the set of
dependencies reachable from the root is identical, so a one-sided entry is not reachable from the root
and cannot change how the root itself executes. It is not entirely inert, though: a package's linkage
table imposes version constraints on packages that later link *it*, so a one-sided entry can still
matter to a downstream consumer, which is why the verifier surfaces it instead of dropping it.

## Security

Verifying source against the wrong on-chain data would let a package look authentic when it is not.
Two failure modes drive the design:

- **The wrong address.** If the verifier compared the source against an address chosen independently
  of the package system, an attacker could arrange for a source to verify against one address while a
  package *depending on it* links a different one — so the "verified" source is not the code that
  runs. The verifier avoids this by reading the address from the package's own publication through
  `read_publication`, the exact resolution the package system performs when linking against the
  package; the two cannot disagree by construction. It also checks that the on-chain package's
  **original id** matches the one the publication records, so the package at that address really is
  the one the source claims to be. A `Published.toml` takes precedence over legacy addresses in both
  the verifier and the package system, so adding one to make an old package verifiable opens no
  discrepancy; and publications are keyed by environment **name**, not chain id, so two environments
  that share a chain id are never confused.

- **The wrong linkage.** Module bytecode does not by itself pin the *versions* of the dependencies a
  package calls into — that is the linkage table's job — so a package whose modules match but whose
  linkage does not is running against different dependency code. Sui's linkage is flat: when a package
  executes, its own root table chooses the version of every transitive dependency it calls into (a
  dependency's table is not ignored in general — it constrains the linkage of packages built on top of
  it — but it does not override the executing root's table). The verifier therefore compares the whole
  table, not just the root's direct dependencies, which is what catches a transitive version swap;
  every entry present on both sides must agree, and an entry on only one side (unreachable from the
  root itself) is reported as a warning.

## What a package must do to be verifiable

Verification reproduces a build, so the build has to be reproducible.

- **Publish from a source you can point at.** The commit that *records* a `published-at` is often not
  the commit that was *published*; any drift shows up as a mismatch in the modules that changed.
- **Pin dependencies to commit hashes.** Lockfiles written by the current package system pin hashes.
  Older ones record the manifest's `rev` verbatim, so a dependency on a branch resolves to whatever
  that branch points at *now*. Such a package cannot be rebuilt reliably; the fix is to pin the
  dependency to a specific hash in the **manifest** (a rebuild then updates the lockfile to match),
  which may mean digging the hash out of history. When a build or comparison fails, every unpinned
  dependency is reported alongside the failure.
- **Record the toolchain version** for the environment published to.

Some toolchains cannot be used at all — a release may pin a framework revision that no longer exists,
for instance. These fail early, suggesting `--toolchain-version` so another toolchain can be tried.
Compiler output is stable across neighbouring releases, so an adjacent version is usually a workable
substitute; a distant one is not, because the bytecode file format changes occasionally.

## Testing

Three layers, by what they need:

- **Unit tests** cover the comparison and the metadata parsing, with no network and no toolchain.
- **Shell tests** (`tests/shell_tests`) publish to a localnet with the current CLI and verify the
  result, including that a tampered source is rejected. They need no downloads.
- **The version matrix** (`tests/version_matrix.rs`, `#[ignore]`d) publishes a fixture with a
  historical binary and verifies it with the current CLI. Modern packages (>= v1.63) go through the
  CLI end to end; legacy packages, which the CLI cannot place onto a localnet's synthetic chain id,
  drive the library directly (see the module docs). `era_matrix` asserts over a curated handful
  spanning both package systems, and additionally verifies the latest release so that a new release
  which breaks interoperability is caught; it runs nightly, out of the required checks, because it
  downloads release archives. `historical_sweep` reports a table over the versions given in
  `SUI_MATRIX_VERSIONS`; it is how the whole release history was swept, and is not run in CI.

  That one-off sweep covered **every mainnet release**. Everything from v1.8.2 to v1.74.1 round-trips
  except three ranges that cannot: v1.8.1 and earlier ship no binary for this platform; v1.25.1–v1.29.2
  cannot deserialize a current node's protocol config, so they cannot transact; and v1.64.1 pins a
  framework revision that no longer exists. The curated `era_matrix` set samples between those gaps.
```
