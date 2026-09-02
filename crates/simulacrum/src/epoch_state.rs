// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;
use std::sync::Arc;

use anyhow::Result;
use sui_config::{
    transaction_deny_config::TransactionDenyConfig, verifier_signing_config::VerifierSigningConfig,
};
use sui_core::{
    accumulators::funds_read::AccountFundsRead,
    transaction_simulation::{SimulationInputLoader, simulate_transaction},
};
use sui_execution::Executor;
use sui_protocol_config::{Chain, ProtocolConfig, ProtocolVersion};
use sui_types::{
    SUI_ACCUMULATOR_ROOT_OBJECT_ID,
    accumulator_root::{AccumulatorObjId, AccumulatorValue, U128},
    base_types::{ObjectRef, SequenceNumber, TransactionDigest},
    coin_reservation::BorrowedCoinReservationResolver,
    committee::{Committee, EpochId},
    digests::ChainIdentifier,
    effects::TransactionEffects,
    error::SuiResult,
    execution_params::ExecutionOrEarlyError,
    gas::SuiGasStatus,
    inner_temporary_store::InnerTemporaryStore,
    metrics::{BytecodeVerifierMetrics, ExecutionMetrics},
    sui_system_state::{
        SuiSystemState, SuiSystemStateTrait,
        epoch_start_sui_system_state::{EpochStartSystemState, EpochStartSystemStateTrait},
    },
    transaction::{
        InputObjectKind, InputObjects, ReceivingObjects, TransactionData, TransactionDataAPI,
        TxValidityCheckContext, VerifiedTransaction,
    },
    transaction_executor::{SimulateTransactionResult, TransactionChecks},
};

use crate::SimulatorStore;

struct SimulatorAccountFundsRead<'a, S>(&'a S);

struct SimulatorInputLoader<'a, S>(&'a S);

pub struct EpochState {
    epoch_start_state: EpochStartSystemState,
    committee: Committee,
    protocol_config: ProtocolConfig,
    execution_metrics: Arc<ExecutionMetrics>,
    bytecode_verifier_metrics: Arc<BytecodeVerifierMetrics>,
    executor: Arc<dyn Executor + Send + Sync>,
    /// Keeps arbitrary simulated packages out of committed execution's VM cache.
    simulation_executor: Arc<dyn Executor + Send + Sync>,
    chain_identifier: ChainIdentifier,
    /// A counter that advances each time we advance the clock in order to ensure that each update
    /// txn has a unique digest. This is reset on epoch changes
    next_consensus_round: u64,
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
        let simulation_executor = sui_execution::executor(&protocol_config, true).unwrap();

        Self {
            epoch_start_state,
            committee,
            protocol_config,
            execution_metrics,
            bytecode_verifier_metrics,
            executor,
            simulation_executor,
            chain_identifier,
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
                sui_types::base_types::SystemObjectVersions::from_latest_in_store(
                    store.backing_store(),
                ),
                gas_data,
                gas_status,
                kind,
                None, // compat_args
                signer,
                tx_digest,
                &mut None,
            );
        Ok((inner_temp_store, gas_status, effects, result))
    }

    pub(crate) fn simulate_transaction<S: SimulatorStore + Send + Sync>(
        &self,
        store: &S,
        transaction_deny_config: &TransactionDenyConfig,
        verifier_signing_config: &VerifierSigningConfig,
        transaction: TransactionData,
        checks: TransactionChecks,
        allow_mock_gas_coin: bool,
    ) -> SuiResult<SimulateTransactionResult> {
        let input_loader = SimulatorInputLoader(store);
        let account_funds_read = SimulatorAccountFundsRead(store);
        let coin_reservation_resolver = BorrowedCoinReservationResolver::new(store);
        let certificate_deny_set = HashSet::new();

        simulate_transaction(
            transaction,
            checks,
            allow_mock_gas_coin,
            Some(self.reference_gas_price()),
            TxValidityCheckContext {
                config: &self.protocol_config,
                epoch: self.epoch(),
                chain_identifier: self.chain_identifier,
                reference_gas_price: self.reference_gas_price(),
                committee_size: self.committee.num_members() as u32,
            },
            self.epoch(),
            self.epoch_start_state.epoch_start_timestamp_ms(),
            self.chain_identifier,
            transaction_deny_config,
            &certificate_deny_set,
            &input_loader,
            store,
            store,
            self.simulation_executor.as_ref(),
            &coin_reservation_resolver,
            &account_funds_read,
            verifier_signing_config,
            &self.bytecode_verifier_metrics,
            &self.execution_metrics,
        )
    }
}

impl<S: SimulatorStore + Send + Sync> SimulatorAccountFundsRead<'_, S> {
    fn account_amount(
        &self,
        account_id: &AccumulatorObjId,
        version: Option<SequenceNumber>,
    ) -> u128 {
        AccumulatorValue::load_by_id::<U128>(self.0, version, *account_id)
            .expect("simulator accumulator reads must succeed")
            .map(|value| value.value)
            .unwrap_or(0)
    }
}

impl<S: SimulatorStore + Send + Sync> AccountFundsRead for SimulatorAccountFundsRead<'_, S> {
    fn get_latest_account_amount(&self, account_id: &AccumulatorObjId) -> u128 {
        self.account_amount(account_id, None)
    }

    fn get_consistent_latest_account_amount_and_version(
        &self,
        account_id: &AccumulatorObjId,
    ) -> (u128, SequenceNumber) {
        let root_version = SimulatorStore::get_object(self.0, &SUI_ACCUMULATOR_ROOT_OBJECT_ID)
            .expect("simulator accumulator root must exist")
            .version();
        (
            self.get_account_amount_at_version(account_id, root_version),
            root_version,
        )
    }

    fn get_account_amount_at_version(
        &self,
        account_id: &AccumulatorObjId,
        version: SequenceNumber,
    ) -> u128 {
        self.account_amount(account_id, Some(version))
    }
}

impl<S: SimulatorStore> SimulationInputLoader for SimulatorInputLoader<'_, S> {
    fn read_objects_for_simulation(
        &self,
        transaction_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_object_refs: &[ObjectRef],
        _epoch_id: EpochId,
    ) -> SuiResult<(InputObjects, ReceivingObjects)> {
        self.0.read_objects_for_synchronous_execution(
            transaction_digest,
            input_object_kinds,
            receiving_object_refs,
        )
    }
}
