// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Spin up a local Sui simulator and run a transactional test against it.
// Spin up an indexer to read data from the simulator
// Spin up a GraphQL server to serve data from the indexer DB
// Spin up a GraphQL client to query data from the GraphQL server

use std::path::Path;
mod simulator_runner;
use move_transactional_test_runner::framework::handle_actual_output;
use simulator_runner::test_adapter::{SuiTestAdapter, PRE_COMPILED};
pub const TEST_DIR: &str = "tests";

#[test]
fn testx() {
    datatest_stable::harness!(run_test, TEST_DIR, r".*\.(mvir|move)$");
}
#[cfg_attr(not(msim), tokio::main)]
#[cfg_attr(msim, msim::main)]
pub async fn run_test(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let output = handle_actual_output::<SuiTestAdapter>(path, Some(&*PRE_COMPILED)).await?;
    Ok(())
}

struct NetworkConfig {
    pub simulator_host: String,
    pub simulator_port: u16,

    pub indexer_db_url: String,

    pub graphql_server_host: String,
    pub graphql_server_port: u16,
}
