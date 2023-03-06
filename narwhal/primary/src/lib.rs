// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
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
mod certificate_fetcher;
mod certifier;
mod grpc_server;
mod primary;
mod proposer;
mod state_handler;
mod synchronizer;
mod utils;

#[cfg(test)]
#[path = "tests/common.rs"]
mod common;

#[cfg(test)]
#[path = "tests/certificate_tests.rs"]
mod certificate_tests;

pub use crate::{
    block_remover::BlockRemover,
    block_synchronizer::{mock::MockBlockSynchronizer, BlockHeader},
    block_waiter::{BlockWaiter, GetBlockResponse},
    primary::{NetworkModel, Primary, CHANNEL_CAPACITY, NUM_SHUTDOWN_RECEIVERS},
};
