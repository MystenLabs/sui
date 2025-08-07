// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use anyhow::Result;
use sui_config::{
    certificate_deny_config::CertificateDenyConfig, transaction_deny_config::TransactionDenyConfig,
    verifier_signing_config::VerifierSigningConfig,
};
use sui_execution::Executor;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    base_types::ObjectID,
    committee::{Committee, EpochId},
    digests::TransactionDigest,
    effects::{TransactionEffects, TransactionEvents},
    error::ExecutionError,
    execution::ExecutionResult,
    execution_params::{ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error},
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    sui_system_state::{
        SuiSystemState, SuiSystemStateTrait,
        epoch_start_sui_system_state::{EpochStartSystemState, EpochStartSystemStateTrait},
    },
    transaction::{ObjectReadResult, TransactionData, TransactionDataAPI, VerifiedTransaction},
    transaction_executor::TransactionChecks,
};

use crate::SimulatorStore;

const DEV_INSPECT_GAS_COIN_VALUE: u64 = 1_000_000_000_000_000_000;

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
        let protocol_config =
            ProtocolConfig::get_for_version(system_state.protocol_version().into(), Chain::Unknown);
        Self::new_with_protocol_config(system_state, protocol_config)
    }

    pub fn new_with_protocol_config(
        system_state: SuiSystemState,
        protocol_config: ProtocolConfig,
    ) -> Self {
        let epoch_start_state = system_state.into_epoch_start_state();
        let committee = epoch_start_state.get_sui_committee();
        let registry = prometheus::Registry::new();
        let limits_metrics = Arc::new(LimitsMetrics::new(&registry));
        let bytecode_verifier_metrics = Arc::new(BytecodeVerifierMetrics::new(&registry));
        let executor = sui_execution::executor(&protocol_config, true).unwrap();

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

    #[allow(clippy::type_complexity)]
    pub fn simulate_transaction_impl(
        &self,
        store: &dyn SimulatorStore,
        deny_config: &TransactionDenyConfig,
        certificate_deny_config: &CertificateDenyConfig,
        verifier_signing_config: &VerifierSigningConfig,
        mut transaction: TransactionData,
        checks: TransactionChecks,
        allow_mock_gas_coin: bool,
    ) -> Result<(
        InnerTemporaryStore,
        TransactionEffects,
        TransactionEvents,
        Result<Vec<ExecutionResult>, ExecutionError>,
        Option<ObjectID>,
    )> {
        let skip_checks = checks.disabled();

        if transaction.kind().is_system_tx() {
            return Err(anyhow::anyhow!(
                "simulate does not support system transactions"
            ));
        }

        transaction.validity_check_no_gas_check(&self.protocol_config)?;

        let input_object_kinds = transaction.input_objects()?;
        let receiving_object_refs = transaction.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            &transaction,
            &[],
            &input_object_kinds,
            &receiving_object_refs,
            deny_config,
            &store,
        )?;

        let mock_gas_object = if allow_mock_gas_coin && transaction.gas().is_empty() {
            let obj = Object::new_move(
                MoveObject::new_gas_coin(
                    OBJECT_START_VERSION,
                    ObjectID::MAX,
                    DEV_INSPECT_GAS_COIN_VALUE,
                ),
                Owner::AddressOwner(transaction.gas_data().owner),
                TransactionDigest::genesis_marker(),
            );
            transaction.gas_data_mut().payment = vec![obj.compute_object_reference()];
            Some(obj)
        } else {
            None
        };

        let tx_digest = transaction.digest();
        let (mut input_objects, receiving_objects) = store.read_objects_for_synchronous_execution(
            &tx_digest,
            &input_object_kinds,
            &receiving_object_refs,
        )?;

        let ((gas_status, checked_input_objects), mock_gas_id) = if skip_checks {
            let mock_gas_id = mock_gas_object.map(|obj| {
                let id = obj.id();
                input_objects.push(ObjectReadResult::new_from_gas_object(&obj));
                id
            });
            let result = sui_transaction_checks::check_dev_inspect_input(
                &self.protocol_config,
                &transaction,
                input_objects,
                receiving_objects,
                self.reference_gas_price(),
            )?;
            (result, mock_gas_id)
        } else if let Some(gas_object) = mock_gas_object {
            let id = gas_object.id();
            let result = sui_transaction_checks::check_transaction_input_with_given_gas(
                &self.protocol_config,
                self.reference_gas_price(),
                &transaction,
                input_objects,
                receiving_objects,
                gas_object,
                &self.bytecode_verifier_metrics,
                verifier_signing_config,
            )?;
            (result, Some(id))
        } else {
            // Run the transaction input checks that would run when submitting the txn to a validator
            // for signing
            let result = sui_transaction_checks::check_transaction_input(
                &self.protocol_config,
                self.reference_gas_price(),
                &transaction,
                input_objects,
                &receiving_objects,
                &self.bytecode_verifier_metrics,
                verifier_signing_config,
            )?;
            (result, None)
        };

        let transaction_digest = transaction.digest();
        let early_execution_error = get_early_execution_error(
            &transaction_digest,
            &checked_input_objects,
            certificate_deny_config.certificate_deny_set(),
            &FundsWithdrawStatus::MaybeSufficient,
        );
        let execution_params = match early_execution_error {
            Some(error) => ExecutionOrEarlyError::Err(error),
            None => ExecutionOrEarlyError::Ok(()),
        };

        let (kind, signer, gas_data) = transaction.execution_parts();
        let (inner_temp_store, _gas_status, effects, execution_result) =
            self.executor.dev_inspect_transaction(
                store.backing_store(),
                &self.protocol_config,
                self.limits_metrics.clone(),
                false, // enable_expensive_checks
                execution_params,
                &self.epoch_start_state.epoch(),
                self.epoch_start_state.epoch_start_timestamp_ms(),
                checked_input_objects,
                gas_data,
                gas_status,
                kind,
                signer,
                transaction_digest,
                skip_checks,
            );

        let events = inner_temp_store.events.clone();
        Ok((
            inner_temp_store,
            effects,
            events,
            execution_result,
            mock_gas_id,
        ))
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
        let (inner_temp_store, gas_status, effects, _timings, result) =
            self.executor.execute_transaction_to_effects(
                store.backing_store(),
                &self.protocol_config,
                self.limits_metrics.clone(),
                false, // enable_expensive_checks
                // TODO: Integrate with early execution error
                ExecutionOrEarlyError::Ok(()),
                &self.epoch_start_state.epoch(),
                self.epoch_start_state.epoch_start_timestamp_ms(),
                checked_input_objects,
                gas_data,
                gas_status,
                kind,
                signer,
                tx_digest,
                &mut None,
            );
        Ok((inner_temp_store, gas_status, effects, result))
    }
}
