// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::BTreeMap;
use std::sync::Arc;

use anyhow::Result;
use sui_execution::Executor;
use sui_protocol_config::ProtocolConfig;
use sui_transaction_checks::check_certificate_input;
use sui_types::{
    base_types::{EpochId, ObjectID, SequenceNumber},
    effects::TransactionEffects,
    executable_transaction::VerifiedExecutableTransaction,
    execution_params::ExecutionOrEarlyError,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::LimitsMetrics,
    transaction::{Transaction, TransactionDataAPI, VerifiedTransaction},
};

use crate::storage::InMemoryObjectStore;

pub struct MinimalExecutor {
    executor: Arc<dyn Executor + Send + Sync>,
    protocol_config: ProtocolConfig,
    limits_metrics: Arc<LimitsMetrics>,
    reference_gas_price: u64,
}

pub struct ExecutionResult {
    pub inner_temp_store: InnerTemporaryStore,
    pub effects: TransactionEffects,
    pub gas_status: SuiGasStatus,
}

impl MinimalExecutor {
    pub fn new(protocol_config: ProtocolConfig, reference_gas_price: u64) -> Result<Self> {
        let executor = sui_execution::executor(&protocol_config, true)?;
        let limits_metrics = Arc::new(LimitsMetrics::new(&prometheus::Registry::new()));

        Ok(Self {
            executor,
            protocol_config,
            limits_metrics,
            reference_gas_price,
        })
    }

    pub fn new_for_testing() -> Result<Self> {
        Self::new(ProtocolConfig::get_for_max_version_UNSAFE(), 1000)
    }

    pub fn execute_transaction(
        &self,
        store: &InMemoryObjectStore,
        transaction: Transaction,
        epoch_id: EpochId,
        epoch_timestamp_ms: u64,
        shared_version_assignments: &BTreeMap<(ObjectID, SequenceNumber), SequenceNumber>,
    ) -> Result<ExecutionResult> {
        let input_object_kinds = transaction.data().intent_message().value.input_objects()?;
        let input_objects = store.read_input_objects(&input_object_kinds, shared_version_assignments)?;

        let executable = VerifiedExecutableTransaction::new_from_quorum_execution(
            VerifiedTransaction::new_unchecked(transaction),
            0,
        );

        let (gas_status, checked_input_objects) = check_certificate_input(
            &executable,
            input_objects,
            &self.protocol_config,
            self.reference_gas_price,
        )?;

        let (kind, signer, gas_data) = executable.transaction_data().execution_parts();

        let (inner_temp_store, gas_status, effects, _timings, execution_error) =
            self.executor.execute_transaction_to_effects(
                store,
                &self.protocol_config,
                self.limits_metrics.clone(),
                false,
                ExecutionOrEarlyError::Ok(()),
                &epoch_id,
                epoch_timestamp_ms,
                checked_input_objects,
                gas_data,
                gas_status,
                kind,
                signer,
                *executable.digest(),
                &mut None,
            );

        if let Err(e) = execution_error {
            anyhow::bail!("Transaction execution failed: {e:?}");
        }

        Ok(ExecutionResult {
            inner_temp_store,
            effects,
            gas_status,
        })
    }

    pub fn protocol_config(&self) -> &ProtocolConfig {
        &self.protocol_config
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.reference_gas_price
    }
}
