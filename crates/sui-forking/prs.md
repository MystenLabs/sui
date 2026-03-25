# PRs workflow

## Forking PR1

- this should have the minimal `sui-forking` crate
- Cargo.toml
- main.rs
- [README.md](http://README.md) file
- poc.md
- prs.md

## Forking PR2 -> implement fetching checkpoint data from graphql
- cleanup and refactor the code in forking-data-store. Remove most of the things that are too early to be used/needed.
- implement fetching verified checkpoint data from graphql
- set up some testing infrastructure for it

## Forking PR3 -> implement store in sui-forking crate with support for checkpoint data
- implement a store in sui-forking crate that has APIs for fetching remote data through forking-data-store, and local data through a filesystem API.

## Forking PR4 -> implement fetching object data for the store
- implement the get object APIs for the store
- fetch system state at a checkpoint

## Forking PR5 -> forked network startup
- instantiate the store and wire it up to simulacrum
- start up the forked network service

## Forking PR6 -> implement transaction execution
- implement transaction execution logic in sui-forking & related gRPC APIs

Future PRs: TBD (probably a PR per line below)
- implement simulate transaction API
- implement rest of gRPC APIs (dynamic fields, epoch, etc)
- implement the forking CLI and related gRPC APIs to interact with the network
- implement Sui CLI impersonating sender support, bypassing signature checks
- implement API to programmatically start and interact with the network
