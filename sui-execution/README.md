# Sui Execution

The `sui-execution` crate is responsible for abstracting access to the
execution layer.  It allows us to isolate big changes to the execution
layer that need to be gated behind protocol config changes, to
minimise the risk of inadvertently changing behaviour that is relevant
for state sync (which would cause a fork).

The Execution Layer include:

- The metered verifier, used during signing.
- The VM, for executing transactions to effects.
- The adapter, that integrates Move into Sui.
- Access to the state as seen by the VM, such as type layout
  resolution.

The specific versions of crates in the execution layer are found in:

- `./sui-execution`, (latest version and cuts/copies of sui-specific
  crates).
- `./external-crates/move/move-execution` (cuts/copies of move-specific
  crates).
- `./external-crates/move` (the latest versions of move-specific
  crates).


## Accessing the Execution Layer

All access to these features from authority (validator and fullnode)
code must be via `sui-execution`, and not by directly depending a
constituent crate.

Code that is exclusively used in other tools (such as the CLI, or
internal tools) are free to depend on a specific version of the
execution layer (typically the `latest` version).

If you are unsure whether your code is part of the authority codebase,
or you are writing a library that is used on validators and fullnodes,
and elsewhere, **default to accessing execution via `sui-execution`.**

Not following this rule can introduce the **potential for forks** when
one part of the authority performs execution according to the
execution layer dictated by the protocol config, and other parts
perform execution according to a version of the execution layer that
is hardcoded in their binary (and may change from release-to-release).

`sui-execution tests::test_encapsulation` is a test that detects
potential breaches of this property.


## Kinds of Cut

There are three kinds of cut:

- `latest`
- versioned snapshots, of the form `vX` where `X` is a number, which
  preserve old behaviour.
- "feature" cuts, where in-progress features are staged, typically
  named for that feature.


### The `latest` cut

Ongoing changes to execution are typically added to the `latest`
versions of their crates, found at.

- `./sui-execution/latest`
- `./external-crates/move/`

This is the version that will be used by the latest versions of our
production networks (`mainnet`, `testnet`).

If this version has been used in production already, changes that
might affect existing behaviour will still need to be
**feature-gated** by the protocol config, otherwise, it can be
modified in-place.

Large changes to execution that are difficult to feature gate warrant
**their own execution version**, see "Making a Cut" below for details
on creating such a cut.


### Version Snapshots

Versioned snapshots, such as `v0`, found at:

- `./sui-execution/v0`
- `./external-crates/move/move-execution/v0`

preserve the existing behaviour of execution.  These should generally
not be modified, because doing so risks changing the effects produced
by existing transactions (which would result in a fork).  Legitimate
reasons to change these crates include:

**Fixing a non-deterministic bug.**  There may be bugs that cause a
fullnode to behave differently during state sync than the network did
during execution.  The only way to fix these bugs is to update the
version of execution that the fullnode will use, which may be a
versioned snapshot.  Note that if there is an existing bug but it is
not deterministic, it should **not** be fixed in older versions.

**Updating interfaces.**  Not all crates that are used by the
execution layer are versioned: Base crates such as
`./crates/sui-types` are shared across all versions.  Changes to
interfaces of base crates will warrant a change to versioned snapshots
to fix their use of those interfaces.  The aim of those changes should
always be to preserve existing behaviour.


### Feature Cuts

It can be worthwhile to develop large features that warrant their own
execution version in a separate cut (i.e. not `latest`).  This allows
`latest` to continue shipping to networks uninterrupted while the new
feature is developed.

**Feature cuts should only be used to process transactions on devnet**
as their behaviour may change from release to release, and only devnet
is wiped regularly enough to accommodate that.  This is done using the
`Chain` parameter of `ProtocolConfig::get_for_version`:

``` rust
impl ProtocolConfig {
    /* ... */
    fn get_for_version_impl(version: ProtocolVersion, chain: Chain) -> Self {
        /* ... */
        match version.0 {
            /* ... */
            42 => {
                let mut cfg = Self::get_for_version_impl(version - 1, chain);
                if ![Chain::Mainnet, Chain::Testnet].contains(&chain) {
                    cfg.execution_version = Some(FEATURE_VERSION)
                }
                cfg
            }
            /* ... */
        }
    }
}
```

To use this flow:

- Make a cut from `latest`, named for your feature, and make changes
  to that version.
- When it is time to release the feature, **make a version snapshot**
  from `latest` to preserve its existing behaviour, merge your feature
  into the new `latest`, and delete your feature.


## Making a Cut

Cuts are always made from `latest`, with the process automated by a
script: `./scripts/execution_layer.py`.  To copy the relevant crates
for a new cut, call:

``` shell
./scripts/execution_layer.py cut <FEATURE>
```

Where `<FEATURE>` is the new feature name.  For a versioned snapshot
this is `vX` where `X` is the current version assigned to `latest`,
whereas for feature cuts, it is the feature's name.

The script can be called with `--dry-run` to print a summary of what
it will do, without actually doing it.


## `sui-execution/src/lib.rs`

The entry-point to the execution crate -- `sui-execution/src/lib.rs`
-- is **automatically generated**.  CI tests will confirm that it has
not been modified manually.  Any modifications should be made in one
of two places:

- `sui-execution/src/lib.template.rs` -- a template file with
  expansion points to be filled in, by
- function `generate_lib` in `scripts/execution_layer.py`, which fills
  them in based on the execution modules in the crate.


## Rebasing Cuts

A cut can be `rebase`-d against `latest` using the following command:

```shell
./scripts/execution_layer.py rebase <FEATURE>

```

This saves all the changes that were made to the cut after it was
made, and replays them on a fresh cut from `latest`.  As a precaution,
it will not run if the working directory is not clean (because if it
goes wrong, it will be harder to recover), but this can be overridden
with `--force`.


## Merging Cuts

Cuts support a rudimentary form of `merge`, using patch files:

```shell
./scripts/execution_layer.py merge <BASE> <FEATURE>
```

The `merge` command attempts to merge the changes from `<FEATURE>`
onto the cut at `<BASE>` (It modifies `<BASE>` and leaves `<FEATURE>`
untouched).  Because it operates using patch files, any conflicts
result in a failure to apply the patch.  This can be resolved in two
ways:

- (Recommended) If merging into `latest`, `rebase` the `<FEATURE>`
  first, which will give you an opportunity to resolve all merge
  conflicts during the rebase, to create a clean patch.
- Use the `--dry-run` option of `merge` to output the patch file
  instead of attempting to apply it, so you can manually modify it
  (cut it into pieces, fix conflicts) before applying it.
