# Move Execution

This directory holds versioned copies of external move crates that are
used during execution on nodes.  Once the effects of execution from a
version of these crates has been committed to on chain, its behaviour
needs to be reproducible at a later date, for nodes to replay
transactions while catching up.


## Snapshots

Development typically happens in `external-crates/move`, where
execution crates are embedded among all other crates.  This is done to
simplify upstreaming changes, as the upstream repo follows the
structure in `external-crates/move`.

Snapshots of past versions are kept in this directory, under a version
name (e.g. `v0`, `v1`, ...).  They are preserved to reproduce past
behaviour on-chain, so they should only be modified when crates they
depend on have been modified, and not modifying them would either
cause them to fail to compile, or to change in behaviour themselves.


## Branches

When working on a long-running feature that production networks should
not be exposed to yet, it is also possible to pre-emptively create a
copy of the move-execution crates, make changes, and only enable them
in `devnet`, by running a special version of the execution crates.

In this mode of operation, we do regularly change the "experimental"
branch of the execution engine, with the understanding that it will
only be used in `devnet` which is wiped regularly.

To productionise these changes (to deploy in `testnet` or `mainnet`),
they will need to be incorporated back into the crates in
`external-crates/move`, which represent the "latest" version of
execution, and from there, they will be preserved in a snapshot.


## Versioning

TODO: To be implemented

The version of these crates that are used is decided by a protocol
config which chooses a version of both the `sui-execution` crates and
the `move-execution` crates.  The versioning logic itself is handled
by the `sui-execution` crate.
