# Sui Execution

The `sui-execution` crate is responsible for abstracting access to the
execution layer.  It allows us to isolate big changes to the execution
layer that need to be gated behind protocol config changes, to
minimise the risk of inadvertantly changing behaviour that is relevant
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
- `./external-crates/move-execution` (cuts/copies of move-specific
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
- `./external-crates/move-execution/v0`

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
- Assign the feature its execution version by counting down from
  `u64::MAX` to find the highest version number that is unassigned in
  the binary -- it is okay to re-use execution versions from past
  features that are no longer present because they are only relevant
  on a version of devnet that has already been wiped.
- When it is time to release the feature, **make a version snapshot**
  from `latest` to preserve its existing behaviour, merge your feature
  into the new `latest`, delete your feature, and the version it was
  assigned.


## Making a Cut

Cuts are always made from `latest`, with the process part automated by
a script: `./scripts/cut_execution_layer.sh`.  To copy the relevant
crates for a new cut, call:

``` shell
./scripts/cut_execution_layer.sh -f <FEATURE>
```

Where `<FEATURE>` is the new feature name.  For a versioned snapshot
this is `vX` where `X` is the current version assigned to `latest`,
whereas for feature cuts, it is the feature's name.

The script can be called with `--dry-run` to print a summary of what
it will do, without actually doing it.

After the crates have been copied, the `sui-execution` crate must be
modified. The following two sections detail the necessary changes, and
[this commit](https://github.com/MystenLabs/sui/commit/060f916698b03655f0647a6ec73c5321edb16459) shows a worked example of the manual changes necessary to cut `v0`.


### Depending on the new crates

`sui-execution` needs to depend on the following new crates:

- `sui-adapter-<FEATURE>`
- `sui-move-natives-<FEATURE>`
- `sui-verifier-<FEATURE>`
- `move-vm-runtime-<FEATURE>`


### New implementations of `Executor` and `Verifier`

`sui-execution` exposes each version of the execution layer through
two traits: `Executor` and `Verifier`.  Each cut is associated with a
module that implements both of these traits.

To add a module for the new feature add `mod <feature>;` to
`./sui-execution/src/lib.rs` and copy `./sui-execution/src/latest.rs`
to `./sui-execution/src/<feature>.rs`

Finally, replace references to modules ending with `_latest` with
references to modules ending in `_<feature>` in the imports of the
copied file, e.g.

```rust
use sui_adapter_latest::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_latest::execution_engine::execute_transaction_to_effects;
use sui_adapter_latest::programmable_transactions;
use sui_adapter_latest::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_latest::all_natives;
use sui_verifier_latest::meter::SuiVerifierMeter;
```

becomes:

```rust
use sui_adapter_<feature>::adapter::{
    default_verifier_config, new_move_vm, run_metered_move_bytecode_verifier,
};
use sui_adapter_<feature>::execution_engine::execute_transaction_to_effects;
use sui_adapter_<feature>::programmable_transactions;
use sui_adapter_<feature>::type_layout_resolver::TypeLayoutResolver;
use sui_move_natives_<feature>::all_natives;
use sui_verifier_<feature>::meter::SuiVerifierMeter;
```



### Assigning versions to implementations

The mapping between execution versions and implementations of
`Executor` and `Verifier` is handled by two top-level functions in the
`sui-execution` crate (`executor` and `verifier`) which switch on
`execution_version` from the supplied protocol config.

When making a new cut, the mappings in both these functions must be
updated:

- **version cuts** replace the mapping from version `X` to `latest`
  with a mapping from `X` to the cut, `vX`.  `latest` gets a new
  mapping, from `X + 1`.

- **feature cuts** introduce a new mapping from a "feature" execution
  version (a high version number, counting down from `u64::MAX`) to
  the feature cut. `latest` is untouched.

Suppose the `verifier` function starts off like this:

``` rust
mod latest;
mod v0;

mod foo;
mod bar;

pub const FEATURE_FOO: u64 = u64::MAX - 0;
pub const FEATURE_BAR: u64 = u64::MAX - 1;

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    match version {
        0 => Box::new(v0::Verifier::new(protocol_config, is_metered, metrics)),
        1 => Box::new(latest::Verifier::new(protocol_config, is_metered, metrics)),

        FEATURE_FOO => Box::new(foo::Verifier::new(protocol_config, is_metered, metrics)),
        FEATURE_BAR => Box::new(bar::Verifier::new(protocol_config, is_metered, metrics)),
        v => panic!("Unsupported execution version {v}"),
    }
}
```

After cutting `v1` it would look like this:

``` rust
mod latest;
mod v0;
mod v1;

mod foo;
mod bar;

pub const FEATURE_FOO: u64 = u64::MAX - 0;
pub const FEATURE_BAR: u64 = u64::MAX - 1;

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    match version {
        0 => Box::new(v0::Verifier::new(protocol_config, is_metered, metrics)),
        1 => Box::new(v1::Verifier::new(protocol_config, is_metered, metrics)),
        2 => Box::new(latest::Verifier::new(protocol_config, is_metered, metrics)),

        FEATURE_FOO => Box::new(foo::Verifier::new(protocol_config, is_metered, metrics)),
        FEATURE_BAR => Box::new(bar::Verifier::new(protocol_config, is_metered, metrics)),
        v => panic!("Unsupported execution version {v}"),
    }
}
```

And after adding a new, `baz` feature, it would look like this:

``` rust
mod latest;
mod v0;
mod v1;

mod foo;
mod bar;
mod baz

pub const FEATURE_FOO: u64 = u64::MAX - 0;
pub const FEATURE_BAR: u64 = u64::MAX - 1;
pub const FEATURE_BAZ: u64 = u64::MAX - 2;

pub fn verifier<'m>(
    protocol_config: &ProtocolConfig,
    is_metered: bool,
    metrics: &'m Arc<BytecodeVerifierMetrics>,
) -> Box<dyn Verifier + 'm> {
    let version = protocol_config.execution_version_as_option().unwrap_or(0);
    match version {
        0 => Box::new(v0::Verifier::new(protocol_config, is_metered, metrics)),
        1 => Box::new(v1::Verifier::new(protocol_config, is_metered, metrics)),
        2 => Box::new(latest::Verifier::new(protocol_config, is_metered, metrics)),

        FEATURE_FOO => Box::new(foo::Verifier::new(protocol_config, is_metered, metrics)),
        FEATURE_BAR => Box::new(bar::Verifier::new(protocol_config, is_metered, metrics)),
        FEATURE_BAZ => Box::new(baz::Verifier::new(protocol_config, is_metered, metrics)),
        v => panic!("Unsupported execution version {v}"),
    }
}
```


## Future Improvements

There are a couple of opportunities to improve the execution cut
process further, for anyone interested:

- The final manual steps of creating a new cut (adding a module with a
  new `Executor` and `Verifier` impl, and hooking it into the
  `executor` and `verifier` functions), could be automated.

- The process of merging a feature cut back into `latest` is fiddly if
  done manually.  It can be done using patch files, so it behaves
  similarly to a `git rebase`, but this is also difficult to get
  right, and could also be automated.
