// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::OnceLock;

use sui_config::NodeConfig;
use tokio::runtime::Runtime;

fn parse_positive_usize_override(name: &'static str) -> Option<usize> {
    match std::env::var(name) {
        Ok(value) => match value.parse::<usize>() {
            Ok(value) if value > 0 => Some(value),
            _ => {
                tracing::warn!(
                    name,
                    value,
                    "runtime thread override must be a positive integer; using the Tokio default"
                );
                None
            }
        },
        Err(std::env::VarError::NotPresent) => None,
        Err(error) => {
            tracing::warn!(
                name,
                %error,
                "unable to read runtime thread override; using the Tokio default"
            );
            None
        }
    }
}

fn sui_node_worker_threads() -> Option<usize> {
    static WORKER_THREADS: OnceLock<Option<usize>> = OnceLock::new();
    *WORKER_THREADS
        .get_or_init(|| parse_positive_usize_override("SUI_NODE_WORKER_THREADS"))
}

fn sui_node_max_blocking_threads() -> Option<usize> {
    static MAX_BLOCKING_THREADS: OnceLock<Option<usize>> = OnceLock::new();
    *MAX_BLOCKING_THREADS
        .get_or_init(|| parse_positive_usize_override("SUI_NODE_MAX_BLOCKING_THREADS"))
}

pub struct SuiRuntimes {
    // Order in this struct is the order in which runtimes are stopped
    pub sui_node: Runtime,
    pub metrics: Runtime,
}

impl SuiRuntimes {
    pub fn new(_confg: &NodeConfig) -> Self {
        let worker_threads = sui_node_worker_threads();
        let max_blocking_threads = sui_node_max_blocking_threads();
        if worker_threads.is_some() || max_blocking_threads.is_some() {
            tracing::info!(
                ?worker_threads,
                ?max_blocking_threads,
                "applying sui-node runtime thread overrides"
            );
        }

        let mut sui_node_builder = tokio::runtime::Builder::new_multi_thread();
        sui_node_builder.thread_name("sui-node-runtime").enable_all();
        if let Some(worker_threads) = worker_threads {
            sui_node_builder.worker_threads(worker_threads);
        }
        if let Some(max_blocking_threads) = max_blocking_threads {
            sui_node_builder.max_blocking_threads(max_blocking_threads);
        }
        let sui_node = sui_node_builder.build().unwrap();
        let metrics = tokio::runtime::Builder::new_multi_thread()
            .thread_name("metrics-runtime")
            .worker_threads(2)
            .enable_all()
            .build()
            .unwrap();

        Self { sui_node, metrics }
    }
}
