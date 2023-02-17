// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use config::WorkerId;
use executor::SubscriberError;
use futures::future::try_join_all;
use futures::stream::FuturesUnordered;
pub use storage::{CertificateStoreCacheMetrics, NodeStorage};
use thiserror::Error;

pub mod execution_state;
pub mod keypair_file;
pub mod metrics;
pub mod primary_node;
pub mod worker_node;

#[derive(Debug, Error, Clone)]
pub enum NodeError {
    #[error("Failure while booting node: {0}")]
    NodeBootstrapError(#[from] SubscriberError),

    #[error("Node is already running")]
    NodeAlreadyRunning,

    #[error("Worker nodes with ids {0:?} already running")]
    WorkerNodesAlreadyRunning(Vec<WorkerId>),
}
