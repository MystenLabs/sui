// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use sui_config::NodeConfig;
use tokio::runtime::Runtime;

pub struct SuiRuntimes {
    // Order in this struct is the order in which runtimes are stopped
    pub sui_node: Runtime,
    pub metrics: Runtime,
}

impl SuiRuntimes {
    pub fn new(_confg: &NodeConfig) -> Self {
        let sui_node = tokio::runtime::Builder::new_multi_thread()
            .thread_name("sui-node-runtime")
            .enable_all()
            .build()
            .unwrap();
        let metrics = tokio::runtime::Builder::new_multi_thread()
            .thread_name("metrics-runtime")
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        Self { sui_node, metrics }
    }
}
