// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
#![warn(
    future_incompatible,
    nonstandard_style,
    rust_2018_idioms,
    rust_2021_compatibility
)]

mod aggregators;
mod block_remover;
pub mod block_synchronizer;
mod block_waiter;
mod certificate_waiter;
mod core;
mod grpc_server;
mod header_waiter;
mod helper;
mod payload_receiver;
mod primary;
mod proposer;
mod state_handler;
mod synchronizer;
mod utils;

#[cfg(test)]
#[path = "tests/common.rs"]
mod common;
mod metrics;

pub use crate::{
    block_remover::{BlockRemover, BlockRemoverCommand, DeleteBatchMessage},
    block_synchronizer::{
        mock::MockBlockSynchronizer,
        responses::{CertificatesResponse, PayloadAvailabilityResponse},
        BlockHeader,
    },
    block_waiter::{BlockCommand, BlockWaiter},
    grpc_server::metrics::EndpointMetrics,
    primary::{NetworkModel, PayloadToken, Primary, PrimaryWorkerMessage, CHANNEL_CAPACITY},
};
