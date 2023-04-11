// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use async_trait::async_trait;
use std::sync::Arc;

use crate::system_state_observer::SystemStateObserver;
use crate::workloads::{Gas, GasCoinConfig};

use crate::workloads::payload::Payload;
use crate::ValidatorProxy;

// This is the maximum gas we will transfer from primary coin into any gas coin
// for running the benchmark
pub const MAX_GAS_FOR_TESTING: u64 = 1_000_000_000_000;

// TODO: get this information from protocol config
// This is the maximum budget that can be set for a transaction. 50 SUI.
pub const MAX_BUDGET: u64 = 50_000_000_000;
// (COIN_BYTES_SIZE * STORAGE_PRICE * STORAGE_UNITS_PER_BYTE)
pub const STORAGE_COST_PER_COIN: u64 = 130 * 76 * 100;
// (COUNTER_BYTES_SIZE * STORAGE_PRICE * STORAGE_UNITS_PER_BYTE)
pub const STORAGE_COST_PER_COUNTER: u64 = 341 * 76 * 100;
/// Used to estimate the budget required for each transaction.
pub const ESTIMATED_COMPUTATION_COST: u64 = 1_000_000;

#[async_trait]
pub trait WorkloadBuilder<T: Payload + ?Sized>: Send + Sync + std::fmt::Debug {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig>;
    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig>;
    async fn build(&self, init_gas: Vec<Gas>, payload_gas: Vec<Gas>) -> Box<dyn Workload<T>>;
}

/// A Workload is used to generate multiple payloads during setup phase with `make_test_payloads()`
/// which are added to a local queue. We execute transactions (the queue is drained based on the
/// target qps i.e. for 100 tps, the queue will be popped 100 times every second) with those payloads
/// and generate new payloads (which are enqueued back to the queue) with the returned effects. The
/// total number of payloads to generate depends on how much transaction throughput we want and the
/// maximum number of transactions we want to have in flight. For instance, for a 100 target_qps and
/// in_flight_ratio of 5, a maximum of 500 transactions is expected to be in flight and that many
/// payloads are created.
#[async_trait]
pub trait Workload<T: Payload + ?Sized>: Send + Sync + std::fmt::Debug {
    async fn init(
        &mut self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    );
    async fn make_test_payloads(
        &self,
        proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<T>>;
}
