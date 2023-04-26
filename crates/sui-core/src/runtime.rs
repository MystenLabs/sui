// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::env;
use std::str::FromStr;
use sui_config::NodeConfig;
use tap::TapFallible;
use tokio::runtime::Runtime;
use tracing::warn;

pub struct SuiRuntimes {
    // Order in this struct is the order in which runtimes are stopped
    pub sui_node: Runtime,
    pub metrics: Runtime,
    pub json_rpc: Runtime,
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

        let worker_thread = env::var("RPC_WORKER_THREAD")
            .ok()
            .and_then(|o| {
                usize::from_str(&o)
                    .tap_err(|e| warn!("Cannot parse RPC_WORKER_THREAD to usize: {e}"))
                    .ok()
            })
            .unwrap_or(num_cpus::get() / 2);

        let json_rpc = tokio::runtime::Builder::new_multi_thread()
            .thread_name("jsonrpc-runtime")
            .worker_threads(worker_thread)
            .enable_all()
            .build()
            .unwrap();
        Self {
            sui_node,
            metrics,
            json_rpc,
        }
    }
}
