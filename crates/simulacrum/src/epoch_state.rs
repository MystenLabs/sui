// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::HashSet, sync::Arc};

use anyhow::Result;
use sui_config::{
    transaction_deny_config::TransactionDenyConfig, verifier_signing_config::VerifierSigningConfig,
};
use sui_execution::Executor;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    committee::{Committee, EpochId},
    effects::TransactionEffects,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::BytecodeVerifierMetrics,
    metrics::LimitsMetrics,
    sui_system_state::{
        epoch_start_sui_system_state::{EpochStartSystemState, EpochStartSystemStateTrait},
        SuiSystemState, SuiSystemStateTrait,
    },
    transaction::{TransactionDataAPI, VerifiedTransaction},
};

use crate::SimulatorStore;

pub struct EpochState {
    epoch_start_state: EpochStartSystemState,
    committee: Committee,
    protocol_config: ProtocolConfig,
    limits_metrics: Arc<LimitsMetrics>,
    bytecode_verifier_metrics: Arc<BytecodeVerifierMetrics>,
    executor: Arc<dyn Executor + Send + Sync>,
    /// A counter that advances each time we advance the clock in order to ensure that each update
    /// txn has a unique digest. This is reset on epoch changes
    next_consensus_round: u64,
}

impl EpochState {
    pub fn new(system_state: SuiSystemState) -> Self {
        let epoch_start_state = system_state.into_epoch_start_state();
        let committee = epoch_start_state.get_sui_committee();
        let protocol_config =
            ProtocolConfig::get_for_version(epoch_start_state.protocol_version(), Chain::Unknown);
        let registry = prometheus::Registry::new();
        let limits_metrics = Arc::new(LimitsMetrics::new(&registry));
        let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(&registry));
        let executor = sui_execution::executor(&protocol_config, true, None).unwrap();

        Self {
            epoch_start_state,
            committee,
            protocol_config,
            limits_metrics,
            bytecode_verifier_metrics,
            executor,
            next_consensus_round: 0,
        }
    }

    pub fn epoch(&self) -> EpochId {
        self.epoch_start_state.epoch()
    }

    pub fn reference_gas_price(&self) -> u64 {
        self.epoch_start_state.reference_gas_price()
    }

    pub fn next_consensus_round(&mut self) -> u64 {
        let round = self.next_consensus_round;
        self.next_consensus_round += 1;
        round
    }

    pub fn committee(&self) -> &Committee {
        &self.committee
    }

    pub fn epoch_start_state(&self) -> &EpochStartSystemState {
        &self.epoch_start_state
    }

    pub fn protocol_version(&self) -> ProtocolVersion {
        self.protocol_config().version
    }

    pub fn protocol_config(&self) -> &ProtocolConfig {
        &self.protocol_config
    }

    pub fn execute_transaction(
        &self,
        store: &dyn SimulatorStore,
        deny_config: &TransactionDenyConfig,
        verifier_signing_config: &VerifierSigningConfig,
        transaction: &VerifiedTransaction,
    ) -> Result<(
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Result<(), sui_types::error::ExecutionError>,
    )> {
        let tx_digest = *transaction.digest();
        let tx_data = &transaction.data().intent_message().value;
        let input_object_kinds = tx_data.input_objects()?;
        let receiving_object_refs = tx_data.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            tx_data,
            transaction.tx_signatures(),
            &input_object_kinds,
            &receiving_object_refs,
            deny_config,
            &store,
        )?;

        let (input_objects, receiving_objects) = store.read_objects_for_synchronous_execution(
            &tx_digest,
            &input_object_kinds,
            &receiving_object_refs,
        )?;

        // Run the transaction input checks that would run when submitting the txn to a validator
        // for signing
        let (gas_status, checked_input_objects) = sui_transaction_checks::check_transaction_input(
            &self.protocol_config,
            self.epoch_start_state.reference_gas_price(),
            transaction.data().transaction_data(),
            input_objects,
            &receiving_objects,
            &self.bytecode_verifier_metrics,
            verifier_signing_config,
        )?;

        let transaction_data = transaction.data().transaction_data();
        let (kind, signer, gas) = transaction_data.execution_parts();
        let (inner_temp_store, gas_status, effects, _timings, result) =
            self.executor.execute_transaction_to_effects(
                store.backing_store(),
                &self.protocol_config,
                self.limits_metrics.clone(),
                false,           // enable_expensive_checks
                &HashSet::new(), // certificate_deny_set
                &self.epoch_start_state.epoch(),
                self.epoch_start_state.epoch_start_timestamp_ms(),
                checked_input_objects,
                gas,
                gas_status,
                kind,
                signer,
                tx_digest,
                &mut None,
            );
        Ok((inner_temp_store, gas_status, effects, result))
    }
}
