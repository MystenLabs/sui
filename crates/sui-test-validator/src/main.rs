// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

fn main() {
    println!("{}", "sui-test-validator binary has been deprecated in favor of sui start, which is a \
                more powerful command that allows you to start the local network with more options. \
There are three options that you can use to replace `sui-test-validator` binary if you need to \
start an indexer and/or a GraphQL locally:
  * use sui-pg binary from a release archive from the GitHub releases page
  * use the backward compatible scripts/sui-test-validator.sh script from the main Sui repository
  * build from source with indexer feature enabled: cargo build --bin sui --features indexer

If you do not require an indexer or a GraphQL service locally, then use the sui binary and its \
sui start command instead of the sui-pg binary or building it from source.

We recommend to migrate to using sui start instead of using the script. To do so, use sui start \
--help to see all the flags and options.

To recreate the exact basic functionality of sui-test-validator (with no args passed), you must \
use the following options
  * --with-faucet --> to start the faucet server on the default host and port
  * --force-regenesis --> to start the local network without persisting the state and from a new \
genesis

You can also use the following options to start the local network with more features:
  * --with-indexer --> to start the indexer on the default host and port. Note that this requires \
a Postgres database to be running locally, or you need to set the different options to connect to a \
remote indexer database.
  * --with-graphql --> to start the GraphQL server on the default host and port");
}
