// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, RwLock};

use anyhow::Result;
use sui_config::{
    transaction_deny_config::TransactionDenyConfig, verifier_signing_config::VerifierSigningConfig,
};
use sui_execution::Executor;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    accumulator_root::{AccumulatorObjId, UnsettledObjectFundsRead},
    base_types::SequenceNumber,
    committee::{Committee, EpochId},
    digests::ChainIdentifier,
    effects::{TransactionEffects, TransactionEffectsAPI},
    execution_params::ExecutionOrEarlyError,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::{BytecodeVerifierMetrics, ExecutionMetrics},
    sui_system_state::{
        SuiSystemState, SuiSystemStateTrait,
        epoch_start_sui_system_state::{EpochStartSystemState, EpochStartSystemStateTrait},
    },
    transaction::{TransactionDataAPI, VerifiedTransaction},
};

use crate::SimulatorStore;

pub struct EpochState {
    epoch_start_state: EpochStartSystemState,
    committee: Committee,
    protocol_config: ProtocolConfig,
    execution_metrics: Arc<ExecutionMetrics>,
    bytecode_verifier_metrics: Arc<BytecodeVerifierMetrics>,
    executor: Arc<dyn Executor + Send + Sync>,
    chain_identifier: ChainIdentifier,
    unsettled_object_withdrawals: UnsettledObjectWithdrawals,
    /// A counter that advances each time we advance the clock in order to ensure that each update
    /// txn has a unique digest. This is reset on epoch changes
    next_consensus_round: u64,
}

#[derive(Default)]
struct UnsettledObjectWithdrawals {
    unsettled_withdraws: RwLock<BTreeMap<AccumulatorObjId, BTreeMap<SequenceNumber, u128>>>,
}

impl UnsettledObjectFundsRead for UnsettledObjectWithdrawals {
    fn get_unsettled_object_withdraw(
        &self,
        account: &AccumulatorObjId,
        accumulator_version: SequenceNumber,
    ) -> u128 {
        self.unsettled_withdraws
            .read()
            .unwrap()
            .get(account)
            .and_then(|withdraws| withdraws.get(&accumulator_version))
            .copied()
            .unwrap_or(0)
    }
}

impl EpochState {
    pub fn new(system_state: SuiSystemState, chain_identifier: ChainIdentifier) -> Self {
        let protocol_config =
            ProtocolConfig::get_for_version(system_state.protocol_version().into(), Chain::Unknown);
        Self::new_with_protocol_config(system_state, protocol_config, chain_identifier)
    }

    pub fn new_with_protocol_config(
        system_state: SuiSystemState,
        protocol_config: ProtocolConfig,
        chain_identifier: ChainIdentifier,
    ) -> Self {
        let epoch_start_state = system_state.into_epoch_start_state();
        let committee = epoch_start_state.get_sui_committee();
        let registry = prometheus::Registry::new();
        let execution_metrics = Arc::new(ExecutionMetrics::new(&registry));
        let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(&registry));
        let executor = sui_execution::executor(&protocol_config, true).unwrap();

        Self {
            epoch_start_state,
            committee,
            protocol_config,
            execution_metrics,
            bytecode_verifier_metrics,
            executor,
            chain_identifier,
            unsettled_object_withdrawals: UnsettledObjectWithdrawals::default(),
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

    pub fn chain_identifier(&self) -> ChainIdentifier {
        self.chain_identifier
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
        let (kind, signer, gas_data) = transaction_data.execution_parts();
        let system_object_versions =
            sui_types::base_types::SystemObjectVersions::from_latest_in_store(
                store.backing_store().as_object_store(),
            );
        let (inner_temp_store, gas_status, effects, _timings, result) = self
            .executor
            .execute_transaction_to_effects_and_execution_error(
                store.backing_store(),
                &self.protocol_config,
                self.execution_metrics.clone(),
                false, // enable_expensive_checks
                // TODO: Integrate with early execution error
                ExecutionOrEarlyError::ok(None),
                &self.epoch_start_state.epoch(),
                self.epoch_start_state.epoch_start_timestamp_ms(),
                checked_input_objects,
                system_object_versions,
                Some(&self.unsettled_object_withdrawals),
                gas_data,
                gas_status,
                kind,
                None, // compat_args
                signer,
                tx_digest,
                &mut None,
            );
        if effects.status().is_ok()
            && !inner_temp_store
                .accumulator_running_max_withdraws
                .is_empty()
            && let Some(accumulator_version) =
                system_object_versions.get(&SUI_ACCUMULATOR_ROOT_OBJECT_ID)
        {
            self.record_unsettled_object_withdraws(tx_data, &effects, accumulator_version.version);
        }
        Ok((inner_temp_store, gas_status, effects, result))
    }

    fn record_unsettled_object_withdraws(
        &self,
        tx_data: &sui_types::transaction::TransactionData,
        effects: &TransactionEffects,
        accumulator_version: SequenceNumber,
    ) {
        let address_funds_reservations: BTreeSet<_> = tx_data
            .process_funds_withdrawals_for_execution(self.chain_identifier)
            .into_keys()
            .collect();
        let mut unsettled_withdraws = self
            .unsettled_object_withdrawals
            .unsettled_withdraws
            .write()
            .unwrap();
        for event in effects.accumulator_events() {
            if address_funds_reservations.contains(&event.accumulator_obj) {
                continue;
            }
            if let Some(amount) = event
                .write
                .get_fund_withdraw_amount()
                .filter(|amount| *amount > 0)
            {
                let entry = unsettled_withdraws
                    .entry(event.accumulator_obj)
                    .or_default()
                    .entry(accumulator_version)
                    .or_default();
                *entry = entry.checked_add(amount).unwrap();
            }
        }
    }
}
