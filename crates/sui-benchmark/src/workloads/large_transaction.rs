// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Benchmark workload that generates transactions of a configurable size, for measuring
//! consensus throughput as a function of block size.

use super::{
    WorkloadBuilderInfo, WorkloadParams,
    workload::{MAX_GAS_FOR_TESTING, Workload, WorkloadBuilder},
};
use crate::drivers::Interval;
use crate::in_memory_wallet::InMemoryWallet;
use crate::system_state_observer::SystemStateObserver;
use crate::workloads::payload::Payload;
use crate::workloads::{Gas, GasCoinConfig, workload::ExpectedFailureType};
use crate::{ExecutionEffects, ValidatorProxy};
use async_trait::async_trait;
use rand::{Rng, SeedableRng, rngs::SmallRng};
use std::sync::Arc;
use sui_types::base_types::SuiAddress;
use sui_types::crypto::get_key_pair;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use sui_types::transaction::{Command, Transaction, TransactionData};
use sui_types::type_input::TypeInput;
use sui_types::utils::to_sender_signed_transaction;

/// Leave room for the transaction envelope (sender, gas, signature, command) under
/// `max_tx_size_bytes`. Measured envelope is well under 1 KB; 8 KB is a safe margin.
const ENVELOPE_HEADROOM_BYTES: u64 = 8 * 1024;

/// Gas units budgeted per transaction, with generous headroom over the expected cost.
const GAS_UNITS_PER_TX: u64 = 1_000_000;

#[derive(Debug)]
pub struct LargeTransactionTestPayload {
    /// Total payload bytes to attach to each transaction.
    payload_bytes: u64,
    sender: SuiAddress,
    state: InMemoryWallet,
    system_state_observer: Arc<SystemStateObserver>,
    rng: SmallRng,
}

impl std::fmt::Display for LargeTransactionTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "large_transaction")
    }
}

impl Payload for LargeTransactionTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        debug_assert!(
            effects.is_ok() || effects.is_cancelled(),
            "Large transactions should never abort: {effects:?}",
        );
        self.state.update(effects);
    }

    fn make_transaction(&mut self) -> Transaction {
        self.create_transaction()
    }

    fn get_failure_type(&self) -> Option<ExpectedFailureType> {
        None
    }
}

impl LargeTransactionTestPayload {
    fn create_transaction(&mut self) -> Transaction {
        let (gas_price, max_pure_arg_size, max_tx_size) = {
            let state = self.system_state_observer.state.borrow();
            let cfg = state
                .protocol_config
                .as_ref()
                .expect("Protocol config not in system state");
            (
                state.reference_gas_price,
                cfg.max_pure_argument_size() as u64,
                cfg.max_tx_size_bytes(),
            )
        };

        // Stay under the per-input size limit, accounting for the BCS length prefix.
        let per_input = max_pure_arg_size.saturating_sub(16);
        let budget = max_tx_size.saturating_sub(ENVELOPE_HEADROOM_BYTES);
        let total = self.payload_bytes.min(budget);

        let account = self.state.account(&self.sender).unwrap();
        let mut builder = ProgrammableTransactionBuilder::new();

        builder.command(Command::MakeMoveVec(Some(TypeInput::U8), vec![]));

        // Attach the payload as separate pure inputs.
        let mut remaining = total;
        while remaining > 0 {
            let n = remaining.min(per_input);
            let mut chunk = vec![0u8; n as usize];
            self.rng.fill(&mut chunk[..]);
            builder.pure_bytes(chunk, /* force_separate */ true);
            remaining -= n;
        }

        let data = TransactionData::new_programmable(
            self.sender,
            vec![account.gas],
            builder.finish(),
            gas_price * GAS_UNITS_PER_TX,
            gas_price,
        );
        let tx = to_sender_signed_transaction(data, account.key());
        // Assert the full payload made it into the transaction.
        debug_assert!(
            bcs::to_bytes(&tx).map(|b| b.len() as u64).unwrap_or(0) >= total,
            "payload did not reach the transaction: requested {total} bytes",
        );
        tx
    }
}

#[derive(Debug)]
pub struct LargeTransactionWorkloadBuilder {
    num_payloads: u64,
    payload_bytes: u64,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for LargeTransactionWorkloadBuilder {
    async fn generate_coin_config_for_init(&self) -> Vec<GasCoinConfig> {
        // No package to publish.
        vec![]
    }

    async fn generate_coin_config_for_payloads(&self) -> Vec<GasCoinConfig> {
        (0..self.num_payloads)
            .map(|_| {
                let (address, keypair) = get_key_pair();
                GasCoinConfig {
                    amount: MAX_GAS_FOR_TESTING,
                    address,
                    keypair: Arc::new(keypair),
                }
            })
            .collect()
    }

    async fn build(
        &self,
        _init_gas: Vec<Gas>,
        payload_gas: Vec<Gas>,
    ) -> Box<dyn Workload<dyn Payload>> {
        Box::<dyn Workload<dyn Payload>>::from(Box::new(LargeTransactionWorkload {
            payload_bytes: self.payload_bytes,
            payload_gas,
        }))
    }
}

impl LargeTransactionWorkloadBuilder {
    pub fn from(
        workload_weight: f32,
        target_qps: u64,
        num_workers: u64,
        in_flight_ratio: u64,
        payload_bytes: u64,
        duration: Interval,
        group: u32,
    ) -> Option<WorkloadBuilderInfo> {
        let target_qps = (workload_weight * target_qps as f32).ceil() as u64;
        let num_workers = (workload_weight * num_workers as f32).ceil() as u64;
        let max_ops = target_qps * in_flight_ratio;
        if max_ops == 0 || num_workers == 0 {
            return None;
        }
        let workload_params = WorkloadParams {
            target_qps,
            num_workers,
            max_ops,
            duration,
            group,
        };
        let workload_builder = Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(
            LargeTransactionWorkloadBuilder {
                num_payloads: max_ops,
                payload_bytes,
            },
        ));
        Some(WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        })
    }
}

#[derive(Debug)]
pub struct LargeTransactionWorkload {
    payload_bytes: u64,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for LargeTransactionWorkload {
    async fn init(
        &mut self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        _system_state_observer: Arc<SystemStateObserver>,
    ) {
        // Nothing to publish or create.
    }

    async fn make_test_payloads(
        &self,
        _execution_proxy: Arc<dyn ValidatorProxy + Sync + Send>,
        _fullnode_proxies: Vec<Arc<dyn ValidatorProxy + Sync + Send>>,
        system_state_observer: Arc<SystemStateObserver>,
    ) -> Vec<Box<dyn Payload>> {
        self.payload_gas
            .iter()
            .enumerate()
            .map(|(i, gas)| {
                Box::<dyn Payload>::from(Box::new(LargeTransactionTestPayload {
                    payload_bytes: self.payload_bytes,
                    sender: gas.1,
                    state: InMemoryWallet::new(gas),
                    system_state_observer: system_state_observer.clone(),
                    rng: SmallRng::seed_from_u64(i as u64),
                }))
            })
            .collect()
    }

    fn name(&self) -> &str {
        "LargeTransaction"
    }
}
