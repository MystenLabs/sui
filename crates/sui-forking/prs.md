# PRs workflow

## Forking PR1

- this should have the minimal `sui-forking` crate
- Cargo.toml
- lib.rs
- [README.md](http://README.md) file
- design.md
- prs.md

## Forking PR2  → start forked network

Goals:

- just the starting aspect, no downloading objects, no creating filesystem files/directories
- allow to fork from checkpoint or if none given, from latest checkpoint
    - this requires fetching protocol version, checkpoint
    - requires also network identification
- spin up simulacrum with its own ServiceStore
- have a test showcasing the status of the forked network
- have a forking store implementation with most APIs marked with todo!()

Testing:

- Have an interface to forking store so that we can mock up the store and allow us to start the forked network with some random checkpoint (maybe mock checkpoint builder can be used here), and check its status.

Note that PR3 moves from using GraphQL to using forking-data-store to download the starting checkpoint. The reason for reworking this PR in PR3 is that it would be too much to unpack in this PR.

## Forking PR3 → start forked network with forking-data-store checkpoint download

- move from using GraphQL to download checkpoints using forking-data-store.
- support fetching checkpoints and serializing them on disk

Testing:

- Set up a mocking forking-data-store based on the interface defined in RP2 that enables downloading checkpoints and verifying them being saved on disk as per the config.

## Forking PR4 → start forked network with fetching objects from GraphQL (via forking-data-store) at the right versions at that forked checkpoint

- Fetching start-up objects
    - 0x1, 0x2, 0x3, 0x5, 0xdee9
- probably here we want to introduce ledger-service gRPC implementation to be able to fetch objects and store them in-memory and on file disk.
- This requires also implementing the relevant code for `forking-data-store`

Testing:

- Check that at startup the objects have the correct version. For this, we probably want something like starting a local test network from which we fork.

## Forking PR5 → execute a basic transaction on the forked network

- Implement the gRPC execution interface
- Store the new objects on disk

Testing:

- this will be basically testing the whole execution workflow, making sure that effects are committed and that new objects are written to memory & disk
- test various transactions

Forking PR6 → Advance checkpoint and clock

Forking PR7 → Advance epoch

Forking PR8 → Start the network with seeded information
- Implement state service gRPC interface
- Implement the required code in forking-data-store to handle live object set (owned objects)

Future PRs: TBD (probably a PR per line below)
- implement subscription service
- implement simulate transaction API
- implement rest of gRPC APIs (dynamic fields, epoch, etc)
- implement the forking CLI
- implement Sui CLI impersonating sender support, bypassing signature checks
- implement API to programmatically start the network and work with it (e.g., for testing purposes)
