// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashSet;

use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_core::accumulators::funds_read::AccountFundsRead;
use sui_core::transaction_simulation::SimulationInputLoader;
use sui_core::transaction_simulation::simulate_transaction;
use sui_types::SUI_ACCUMULATOR_ROOT_OBJECT_ID;
use sui_types::accumulator_root::AccumulatorObjId;
use sui_types::accumulator_root::AccumulatorValue;
use sui_types::accumulator_root::U128;
use sui_types::base_types::EpochId;
use sui_types::base_types::ObjectRef;
use sui_types::base_types::SequenceNumber;
use sui_types::base_types::TransactionDigest;
use sui_types::coin_reservation::BorrowedCoinReservationResolver;
use sui_types::error::SuiResult;
use sui_types::sui_system_state::epoch_start_sui_system_state::EpochStartSystemStateTrait;
use sui_types::transaction::InputObjectKind;
use sui_types::transaction::InputObjects;
use sui_types::transaction::ReceivingObjects;
use sui_types::transaction::TransactionData;
use sui_types::transaction::TxValidityCheckContext;
use sui_types::transaction_executor::SimulateTransactionResult;
use sui_types::transaction_executor::TransactionChecks;

use crate::SimulatorStore;

use super::EpochState;

struct SimulatorAccountFundsRead<'a, S>(&'a S);

struct SimulatorInputLoader<'a, S>(&'a S);

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

impl EpochState {
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
