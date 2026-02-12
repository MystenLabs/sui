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
    effects::TransactionEffects,
    execution_params::{ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error},
    gas::SuiGasStatus,
    inner_temporary_store::{InnerTemporaryStore, PackageStoreWithFallback},
    layout_resolver::LayoutResolver,
    metrics::{BytecodeVerifierMetrics, LimitsMetrics},
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
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

    /// Creates a layout resolver for use with dev_inspect results.
    /// Call this after `dev_inspect_transaction_block` to get a layout resolver.
    pub fn create_layout_resolver<'a>(
        &'a self,
        store: &'a dyn SimulatorStore,
        inner_temp_store: &'a InnerTemporaryStore,
    ) -> Box<dyn LayoutResolver + 'a> {
        self.executor
            .type_layout_resolver(Box::new(PackageStoreWithFallback::new(
                inner_temp_store,
                store.backing_store(),
            )))
    }

    /// The object ID for gas can be any object ID, even for an uncreated object
    #[allow(clippy::collapsible_else_if)]
    // pub fn dev_inspect_transaction_block(
    //     &self,
    //     store: &dyn SimulatorStore,
    //     sender: SuiAddress,
    //     transaction_kind: TransactionKind,
    //     gas_price: Option<u64>,
    //     gas_budget: Option<u64>,
    //     gas_sponsor: Option<SuiAddress>,
    //     gas_objects: Option<Vec<ObjectRef>>,
    //     show_raw_txn_data_and_effects: Option<bool>,
    //     skip_checks: Option<bool>,
    //     deny_config: &TransactionDenyConfig,
    //     certificate_deny_config: &CertificateDenyConfig,
    //     verifier_signing_config: &VerifierSigningConfig,
    // ) -> anyhow::Result<(
    //     InnerTemporaryStore,
    //     TransactionEffects,
    //     TransactionEvents,
    //     Vec<u8>, /* raw txn data */
    //     Vec<u8>, /* raw_effects */
    //     Result<Vec<ExecutionResult>, ExecutionError>,
    // )> {
    //     if transaction_kind.is_system_tx() {
    //         return Err(SuiErrorKind::UnsupportedFeatureError {
    //             error: "system transactions are not supported".to_string(),
    //         }
    //         .into());
    //     }
    //
    //     let show_raw_txn_data_and_effects = show_raw_txn_data_and_effects.unwrap_or(false);
    //     let skip_checks = skip_checks.unwrap_or(true);
    //     let reference_gas_price = self.reference_gas_price();
    //     let protocol_config = self.protocol_config();
    //     let max_tx_gas = protocol_config.max_tx_gas();
    //
    //     let price = gas_price.unwrap_or(reference_gas_price);
    //     let budget = gas_budget.unwrap_or(max_tx_gas);
    //     let owner = gas_sponsor.unwrap_or(sender);
    //     // Payment might be empty here, but it's fine we'll have to deal with it later after reading all the input objects.
    //     let payment = gas_objects.unwrap_or_default();
    //     let mut transaction = TransactionData::V1(TransactionDataV1 {
    //         kind: transaction_kind.clone(),
    //         sender,
    //         gas_data: GasData {
    //             payment,
    //             owner,
    //             price,
    //             budget,
    //         },
    //         expiration: TransactionExpiration::None,
    //     });
    //
    //     let raw_txn_data = if show_raw_txn_data_and_effects {
    //         bcs::to_bytes(&transaction).map_err(|_| {
    //             SuiErrorKind::TransactionSerializationError {
    //                 error: "Failed to serialize transaction during dev inspect".to_string(),
    //             }
    //         })?
    //     } else {
    //         vec![]
    //     };
    //
    //     transaction.validity_check_no_gas_check(protocol_config)?;
    //
    //     let input_object_kinds = transaction.input_objects()?;
    //     let receiving_object_refs = transaction.receiving_objects();
    //
    //     sui_transaction_checks::deny::check_transaction_for_signing(
    //         &transaction,
    //         &[],
    //         &input_object_kinds,
    //         &receiving_object_refs,
    //         deny_config,
    //         &store,
    //     )?;
    //
    //     // TODO forking: replace this with proper way for reading input/receiving objects.
    //     // let (mut input_objects, receiving_objects) = self.input_loader.read_objects_for_signing(
    //     //     // We don't want to cache this transaction since it's a dev inspect.
    //     //     None,
    //     //     &input_object_kinds,
    //     //     &receiving_object_refs,
    //     //     self.epoch(),
    //     // )?;
    //
    //     let tx_digest = transaction.digest();
    //     let (mut input_objects, receiving_objects) = store.read_objects_for_synchronous_execution(
    //         &tx_digest,
    //         &input_object_kinds,
    //         &receiving_object_refs,
    //     )?;
    //
    //     let (gas_status, checked_input_objects) = if skip_checks {
    //         // If we are skipping checks, then we call the check_dev_inspect_input function which will perform
    //         // only lightweight checks on the transaction input. And if the gas field is empty, that means we will
    //         // use the dummy gas object so we need to add it to the input objects vector.
    //         if transaction.gas().is_empty() {
    //             // Create and use a dummy gas object if there is no gas object provided.
    //             let dummy_gas_object = Object::new_gas_with_balance_and_owner_for_testing(
    //                 DEV_INSPECT_GAS_COIN_VALUE,
    //                 transaction.gas_owner(),
    //             );
    //             let gas_object_ref = dummy_gas_object.compute_object_reference();
    //             transaction.gas_data_mut().payment = vec![gas_object_ref];
    //             input_objects.push(ObjectReadResult::new(
    //                 InputObjectKind::ImmOrOwnedMoveObject(gas_object_ref),
    //                 dummy_gas_object.into(),
    //             ));
    //         }
    //         let checked_input_objects = sui_transaction_checks::check_dev_inspect_input(
    //             protocol_config,
    //             &transaction_kind,
    //             input_objects,
    //             receiving_objects,
    //
    //         )?;
    //         let gas_status = SuiGasStatus::new(
    //             max_tx_gas,
    //             transaction.gas_price(),
    //             reference_gas_price,
    //             protocol_config,
    //         )?;
    //
    //         (gas_status, checked_input_objects)
    //     } else {
    //         // If we are not skipping checks, then we call the check_transaction_input function and its dummy gas
    //         // variant which will perform full fledged checks just like a real transaction execution.
    //         if transaction.gas().is_empty() {
    //             // Create and use a dummy gas object if there is no gas object provided.
    //             let dummy_gas_object = Object::new_gas_with_balance_and_owner_for_testing(
    //                 DEV_INSPECT_GAS_COIN_VALUE,
    //                 transaction.gas_owner(),
    //             );
    //             let gas_object_ref = dummy_gas_object.compute_object_reference();
    //             transaction.gas_data_mut().payment = vec![gas_object_ref];
    //             sui_transaction_checks::check_transaction_input_with_given_gas(
    //                 self.protocol_config(),
    //                 self.reference_gas_price(),
    //                 &transaction,
    //                 input_objects,
    //                 receiving_objects,
    //                 dummy_gas_object,
    //                 &self.bytecode_verifier_metrics,
    //                 verifier_signing_config,
    //             )?
    //         } else {
    //             sui_transaction_checks::check_transaction_input(
    //                 self.protocol_config(),
    //                 self.reference_gas_price(),
    //                 &transaction,
    //                 input_objects,
    //                 &receiving_objects,
    //                 &self.bytecode_verifier_metrics,
    //                 &verifier_signing_config,
    //             )?
    //         }
    //     };
    //
    //     let gas_data = transaction.gas_data().clone();
    //     let intent_msg = IntentMessage::new(
    //         Intent {
    //             version: IntentVersion::V0,
    //             scope: IntentScope::TransactionData,
    //             app_id: AppId::Sui,
    //         },
    //         transaction,
    //     );
    //     let transaction_digest = TransactionDigest::new(default_hash(&intent_msg.value));
    //     let early_execution_error = get_early_execution_error(
    //         &transaction_digest,
    //         &checked_input_objects,
    //         certificate_deny_config.certificate_deny_set(),
    //         // TODO(address-balances): Mimic withdraw scheduling and pass the result.
    //         &FundsWithdrawStatus::MaybeSufficient,
    //     );
    //     let execution_params = match early_execution_error {
    //         Some(error) => ExecutionOrEarlyError::Err(error),
    //         None => ExecutionOrEarlyError::Ok(()),
    //     };
    //     let (inner_temp_store, _, effects, execution_result) =
    //         self.executor.dev_inspect_transaction(
    //             store.backing_store(),
    //             &self.protocol_config,
    //             self.limits_metrics.clone(),
    //             /* expensive checks */ false,
    //             execution_params,
    //             &self.epoch_start_state.epoch(),
    //             self.epoch_start_state.epoch_start_timestamp_ms(),
    //             checked_input_objects,
    //             gas_data,
    //             gas_status,
    //             transaction_kind,
    //             sender,
    //             transaction_digest,
    //             skip_checks,
    //         );
    //
    //     let raw_effects = if show_raw_txn_data_and_effects {
    //         bcs::to_bytes(&effects).map_err(|_| SuiErrorKind::TransactionSerializationError {
    //             error: "Failed to serialize transaction effects during dev inspect".to_string(),
    //         })?
    //     } else {
    //         vec![]
    //     };
    //
    //     let events = inner_temp_store.events.clone();
    //     Ok((
    //         inner_temp_store,
    //         effects,
    //         events,
    //         raw_txn_data,
    //         raw_effects,
    //         execution_result,
    //     ))
    // }
    #[allow(clippy::type_complexity)]
    pub fn dry_run_exec_impl(
        &self,
        store: &dyn SimulatorStore,
        deny_config: &TransactionDenyConfig,
        certificate_deny_config: &CertificateDenyConfig,
        verifier_signing_config: &VerifierSigningConfig,
        transaction: &VerifiedTransaction,
        transaction_digest: &TransactionDigest,
    ) -> Result<(
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Option<ObjectID>,
        Result<(), sui_types::error::ExecutionError>,
    )> {
        let tx_digest = *transaction.digest();
        let tx_data = &transaction.data().intent_message().value;
        let input_object_kinds = tx_data.input_objects()?;
        let receiving_object_refs = tx_data.receiving_objects();

        sui_transaction_checks::deny::check_transaction_for_signing(
            tx_data,
            &[],
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

        // make a gas object if one was not provided
        let mut gas_data = transaction.transaction_data().gas_data().clone();
        let ((gas_status, checked_input_objects), mock_gas) = if transaction.gas().is_empty() {
            let sender = transaction.transaction_data().sender();
            // use a 1B sui coin
            const MIST_TO_SUI: u64 = 1_000_000_000;
            const DRY_RUN_SUI: u64 = 1_000_000_000;
            let max_coin_value = MIST_TO_SUI * DRY_RUN_SUI;
            let gas_object_id = ObjectID::random();
            let gas_object = Object::new_move(
                MoveObject::new_gas_coin(OBJECT_START_VERSION, gas_object_id, max_coin_value),
                Owner::AddressOwner(sender),
                TransactionDigest::genesis_marker(),
            );
            let gas_object_ref = gas_object.compute_object_reference();
            gas_data.payment = vec![gas_object_ref];
            (
                sui_transaction_checks::check_transaction_input_with_given_gas(
                    &self.protocol_config,
                    self.reference_gas_price(),
                    transaction.transaction_data(),
                    input_objects,
                    receiving_objects,
                    gas_object,
                    &self.bytecode_verifier_metrics,
                    verifier_signing_config,
                )?,
                Some(gas_object_id),
            )
        } else {
            (
                sui_transaction_checks::check_transaction_input(
                    &self.protocol_config,
                    self.epoch_start_state.reference_gas_price(),
                    transaction.data().transaction_data(),
                    input_objects,
                    &receiving_objects,
                    &self.bytecode_verifier_metrics,
                    verifier_signing_config,
                )?,
                None,
            )
        };

        let early_execution_error = get_early_execution_error(
            transaction_digest,
            &checked_input_objects,
            certificate_deny_config.certificate_deny_set(),
            // TODO(address-balances): This does not currently support balance withdraws properly.
            // For address balance withdraws, this cannot detect insufficient balance. We need to
            // first check if the balance is sufficient, similar to how we schedule withdraws.
            // For object balance withdraws, we need to handle the case where object balance is
            // insufficient in the post-execution.
            &FundsWithdrawStatus::MaybeSufficient,
        );
        let execution_params = match early_execution_error {
            Some(error) => ExecutionOrEarlyError::Err(error),
            None => ExecutionOrEarlyError::Ok(()),
        };

        let transaction_data = transaction.data().transaction_data();
        let (kind, signer, gas_data) = transaction_data.execution_parts();
        let (inner_temp_store, gas_status, effects, _timings, result) =
            self.executor.execute_transaction_to_effects(
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
                tx_digest,
                &mut None,
            );

        Ok((inner_temp_store, gas_status, effects, mock_gas, result))
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

        println!("Input objects: {input_objects:#?}");

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

        println!("Checked input objects completed");

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
