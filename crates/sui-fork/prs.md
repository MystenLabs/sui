# PRs workflow

## Forking PR1

- this should have the minimal `sui-fork` crate
- Cargo.toml
- main.rs
- [README.md](http://README.md) file
- poc.md
- prs.md

## Forking PR2 -> implement fetching checkpoint data from graphql
- cleanup and refactor the code in forking-data-store. Remove most of the things that are too early to be used/needed.
- implement fetching verified checkpoint data from graphql
- set up some testing infrastructure for it

## Forking PR3 -> implement store in sui-fork crate with support for checkpoint and object data
- implement a store in sui-fork crate that has APIs for fetching data from RPC, and local data through a filesystem API.
- migrate forking-data-store into sui-fork crate, and remove the old crate

## Forking PR4 -> forked network startup
- instantiate the store and wire it up to simulacrum
- start up the forked network service

## Forking PR5 -> implement transaction execution
- implement transaction execution logic in sui-fork & related gRPC APIs

Future PRs: TBD (probably a PR per line below)
- implement simulate transaction API
- implement rest of gRPC APIs (dynamic fields, epoch, etc)
- implement the forking CLI and related gRPC APIs to interact with the network
- implement Sui CLI impersonating sender support, bypassing signature checks
- implement API to programmatically start and interact with the network
