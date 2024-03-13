// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

mod local_authority;
mod local_transaction_execution;

use std::env::temp_dir;
use std::sync::Arc;
use sui_core::storage::RocksDbStore;
use sui_node::build_http_server;

fn main() {
    // 1. Initialize the new kind of store
    // 2. Make http server and register
    //    - read API module
    //    - new transaction execution API

    // let store = Arc::new(RocksDBStore::new(&Some(temp_dir().into_path())));
    //
    // let http_server = build_http_server(
    //     state.clone(),
    //     state_sync_store,
    //     &transaction_orchestrator.clone(),
    //     &config,
    //     &prometheus_registry,
    //     custom_rpc_runtime,
    //     software_version,
    // )?;
}
