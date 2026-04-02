# PRs workflow

## Forking PR1

- this should have the minimal `sui-forking` crate
- Cargo.toml
- lib.rs
- [README.md](http://README.md) file
- design.md
- prs.md

## Forking PR2 -> update forking-data-store
- rework the forking-data-store's traits and APIs
- implement APIs for FileSystemStore and GraphQLStore for Checkpoint data
- set up some testing infrastructure for it

## Forking PR3 -> add gRPC for checkpoint data
- gRPC server implementation for Checkpoint data

## Forking PR4 -> fetch system state at checkpoint
- before we can initialize simulacrum, we need to be able to fetch system state at a checkpoint
- implement the get object APIs

## Forking PR5 -> instantiate simulacrum with forking-data-store store
- use the forking-data-store for the simulacrum instantiation
- implement the checkpoint related APIs to initialize the forked network

## Forking PR6 -> Implement transaction execution
- implement transaction execution logic in sui-forking & related gRPC APIs

Future PRs: TBD (probably a PR per line below)
- implement subscription service
- implement simulate transaction API
- implement rest of gRPC APIs (dynamic fields, epoch, etc)
- implement the forking CLI and related gRPC APIs to interact with the network
- implement Sui CLI impersonating sender support, bypassing signature checks
- implement API to programmatically start and interact with the network
