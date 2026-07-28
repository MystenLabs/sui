// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Byte-only load: transactions that are expensive to *disseminate* and free to *execute*.
//!
//! Consensus carries every transaction byte to every other validator, so block bytes pace
//! the round rate. Isolating that cost requires payload with no execution cost attached.
//! This workload attaches the payload as `Pure` inputs that no command references: they are
//! serialized, signed, and disseminated, but never deserialized or executed. The single
//! command is an empty `MakeMoveVec`, which creates an empty vector in the VM and writes
//! nothing.
//!
//! Two properties of the payload matter and are easy to get wrong:
//!  - Inputs must be *distinct*. `ProgrammableTransactionBuilder` dedupes `Pure` inputs by
//!    content, so identical chunks collapse into one input and the transaction carries the
//!    payload once. We use `force_separate` and unique content.
//!  - Bytes must be *incompressible*. Consensus compresses on the wire, so zero-filled
//!    padding understates real dissemination cost by roughly an order of magnitude.

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
use sui_types::type_input::TypeInput;
use sui_types::transaction::{Command, Transaction, TransactionData};
use sui_types::utils::to_sender_signed_transaction;

/// Leave room for the transaction envelope (sender, gas, signature, command) under
/// `max_tx_size_bytes`. Measured envelope is well under 1 KB; 8 KB is a safe margin.
const ENVELOPE_HEADROOM_BYTES: u64 = 8 * 1024;

/// Gas units budgeted per transaction. These transactions create nothing and run one empty
/// `MakeMoveVec`, so the real cost is the protocol's fixed floor (`base_tx_cost_fixed`,
/// 110k units); this leaves ~9x headroom. Note the payload itself is *free* in gas — the
/// v2 gas model charges no per-byte transaction cost — which is exactly the asymmetry that
/// makes byte load cheap to produce and expensive to disseminate.
const GAS_UNITS_PER_TX: u64 = 1_000_000;

#[derive(Debug)]
pub struct InertBytesTestPayload {
    /// Total payload bytes to attach to each transaction.
    payload_bytes: u64,
    sender: SuiAddress,
    state: InMemoryWallet,
    system_state_observer: Arc<SystemStateObserver>,
    rng: SmallRng,
}

impl std::fmt::Display for InertBytesTestPayload {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "inert_bytes")
    }
}

impl Payload for InertBytesTestPayload {
    fn make_new_payload(&mut self, effects: &ExecutionEffects) {
        debug_assert!(
            effects.is_ok() || effects.is_cancelled(),
            "Inert byte transactions should never abort: {effects:?}",
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

impl InertBytesTestPayload {
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

        // BCS length prefix on a byte vector of this size; stay strictly under the per-input
        // limit, which is checked with `<` rather than `<=`.
        let per_input = max_pure_arg_size.saturating_sub(16);
        let budget = max_tx_size.saturating_sub(ENVELOPE_HEADROOM_BYTES);
        let total = self.payload_bytes.min(budget);

        let account = self.state.account(&self.sender).unwrap();
        let mut builder = ProgrammableTransactionBuilder::new();

        // Free no-op: an empty `MakeMoveVec` is explicitly permitted when the type is given,
        // creates a VM-only value, and writes nothing.
        builder.command(Command::MakeMoveVec(Some(TypeInput::U8), vec![]));

        // Unreferenced payload inputs. `force_separate` defeats content dedup; random fill
        // defeats wire compression.
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
        // The payload only applies dissemination pressure if it actually reaches the wire.
        // `ProgrammableTransactionBuilder` dedupes Pure inputs by content, so non-distinct
        // chunks silently collapse into one input and transactions come out tiny.
        debug_assert!(
            bcs::to_bytes(&tx).map(|b| b.len() as u64).unwrap_or(0) >= total,
            "inert payload did not reach the transaction: requested {total} bytes",
        );
        tx
    }
}

#[derive(Debug)]
pub struct InertBytesWorkloadBuilder {
    num_payloads: u64,
    payload_bytes: u64,
}

#[async_trait]
impl WorkloadBuilder<dyn Payload> for InertBytesWorkloadBuilder {
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
        Box::<dyn Workload<dyn Payload>>::from(Box::new(InertBytesWorkload {
            payload_bytes: self.payload_bytes,
            payload_gas,
        }))
    }
}

impl InertBytesWorkloadBuilder {
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
        let workload_builder =
            Box::<dyn WorkloadBuilder<dyn Payload>>::from(Box::new(InertBytesWorkloadBuilder {
                num_payloads: max_ops,
                payload_bytes,
            }));
        Some(WorkloadBuilderInfo {
            workload_params,
            workload_builder,
        })
    }
}

#[derive(Debug)]
pub struct InertBytesWorkload {
    payload_bytes: u64,
    pub payload_gas: Vec<Gas>,
}

#[async_trait]
impl Workload<dyn Payload> for InertBytesWorkload {
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
                Box::<dyn Payload>::from(Box::new(InertBytesTestPayload {
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
        "InertBytes"
    }
}
