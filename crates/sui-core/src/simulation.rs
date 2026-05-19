// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::collections::{BTreeSet, HashSet};
use std::sync::Arc;

use sui_config::transaction_deny_config::TransactionDenyConfig;
use sui_config::verifier_signing_config::VerifierSigningConfig;
use sui_protocol_config::ProtocolConfig;
use sui_types::base_types::{EpochId, ObjectID, ObjectRef, TransactionDigest};
use sui_types::coin_reservation::{CoinReservationResolverTrait, ParsedDigest};
use sui_types::digests::ChainIdentifier;
use sui_types::effects::TransactionEffectsAPI;
use sui_types::error::{SuiErrorKind, SuiResult};
use sui_types::execution_params::{
    ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error,
};
use sui_types::execution_status::ExecutionErrorKind;
use sui_types::gas::SuiGasStatus;
use sui_types::metrics::{BytecodeVerifierMetrics, ExecutionMetrics};
use sui_types::object::{MoveObject, OBJECT_START_VERSION, Object, Owner};
use sui_types::storage::{BackingPackageStore, BackingStore, TrackingBackingStore};
use sui_types::transaction::{
    InputObjectKind, InputObjects, ObjectReadResult, ReceivingObjects, TransactionData,
    TransactionDataAPI,
};
use sui_types::transaction_executor::{SimulateTransactionResult, TransactionChecks};

use crate::accumulators::funds_read::AccountFundsRead;
use crate::accumulators::transaction_rewriting::rewrite_transaction_for_coin_reservations;
use crate::authority::{DEV_INSPECT_GAS_COIN_VALUE, pre_object_load_checks};

/// Unconditional dependencies needed by transaction simulation.
///
/// Keep this as plain data so callers that are not authority-backed can provide equivalent
/// services without pretending to own an `AuthorityState`.
pub struct SimulateTransactionContext<'a> {
    pub protocol_config: &'a ProtocolConfig,
    pub reference_gas_price: u64,
    pub epoch_id: EpochId,
    pub epoch_timestamp_ms: u64,
    pub chain_identifier: ChainIdentifier,
    pub transaction_deny_config: &'a TransactionDenyConfig,
    pub verifier_signing_config: &'a VerifierSigningConfig,
    pub certificate_deny_set: &'a HashSet<TransactionDigest>,
    pub bytecode_verifier_metrics: Arc<BytecodeVerifierMetrics>,
    pub execution_metrics: Arc<ExecutionMetrics>,
    pub package_store: &'a dyn BackingPackageStore,
    pub backing_store: &'a dyn BackingStore,
    pub coin_reservation_resolver: &'a dyn CoinReservationResolverTrait,
    pub account_funds_read: &'a dyn AccountFundsRead,
}

/// Caller-specific behavior that simulation cannot derive from plain context data.
pub trait SimulateTransactionReader {
    fn read_inputs_for_simulation(
        &self,
        tx_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_object_refs: &[ObjectRef],
    ) -> SuiResult<(InputObjects, ReceivingObjects)>;

    fn suggested_gas_price(&self, _transaction: &TransactionData) -> Option<u64> {
        None
    }
}

#[derive(Debug, Copy, Clone)]
pub struct SimulateTransactionOptions {
    pub checks: TransactionChecks,
    pub allow_mock_gas_coin: bool,
}

pub fn simulate_transaction_with_context<R: SimulateTransactionReader + ?Sized>(
    context: &SimulateTransactionContext<'_>,
    reader: &R,
    mut transaction: TransactionData,
    options: SimulateTransactionOptions,
) -> SuiResult<SimulateTransactionResult> {
    // Reject coin reservations in gas payment when the execution engine
    // doesn't support them.
    if !context.protocol_config.enable_coin_reservation_obj_refs()
        && transaction
            .gas()
            .iter()
            .any(|obj_ref| ParsedDigest::is_coin_reservation_digest(&obj_ref.2))
    {
        return Err(SuiErrorKind::UnsupportedFeatureError {
            error: "coin reservations in gas payment are not supported at this protocol version"
                .to_string(),
        }
        .into());
    }

    // Cheap validity checks for a transaction, including input size limits.
    transaction.validity_check_no_gas_check(context.protocol_config)?;

    let input_object_kinds = transaction.input_objects()?;
    let receiving_object_refs = transaction.receiving_objects();

    // Create and inject mock gas coin before pre_object_load_checks so that
    // funds withdrawal processing sees non-empty payment and doesn't incorrectly
    // create an address balance withdrawal for gas.
    // Skip mock gas for gasless transactions; they don't use gas coins.
    let is_gasless =
        context.protocol_config.enable_gasless() && transaction.is_gasless_transaction();
    let mock_gas_object =
        if options.allow_mock_gas_coin && transaction.gas().is_empty() && !is_gasless {
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

    let declared_withdrawals = pre_object_load_checks(
        &transaction,
        &[],
        &input_object_kinds,
        &receiving_object_refs,
        context.protocol_config,
        context.transaction_deny_config,
        context.package_store,
        context.chain_identifier,
        context.coin_reservation_resolver,
        context.account_funds_read,
    )?;
    let address_funds: BTreeSet<_> = declared_withdrawals.keys().cloned().collect();
    let tx_digest = transaction.digest();

    let (mut input_objects, receiving_objects) = reader.read_inputs_for_simulation(
        &tx_digest,
        &input_object_kinds,
        &receiving_object_refs,
    )?;

    // Add mock gas to input objects after loading; it does not exist in the backing store.
    let mock_gas_id = mock_gas_object.map(|obj| {
        let id = obj.id();
        input_objects.push(ObjectReadResult::new_from_gas_object(&obj));
        id
    });

    let dev_inspect = options.checks.disabled();
    let (gas_status, checked_input_objects) = if dev_inspect {
        sui_transaction_checks::check_dev_inspect_input(
            context.protocol_config,
            &transaction,
            input_objects,
            receiving_objects,
            context.reference_gas_price,
        )?
    } else {
        sui_transaction_checks::check_transaction_input(
            context.protocol_config,
            context.reference_gas_price,
            &transaction,
            input_objects,
            &receiving_objects,
            &context.bytecode_verifier_metrics,
            context.verifier_signing_config,
        )?
    };

    // TODO see if we can spin up a VM once and reuse it
    let executor = sui_execution::executor(
        context.protocol_config,
        true, // silent
    )
    .expect("Creating an executor should not fail here");

    let (mut kind, signer, gas_data) = transaction.execution_parts();
    let rewritten_inputs = rewrite_transaction_for_coin_reservations(
        context.chain_identifier,
        context.coin_reservation_resolver,
        signer,
        &mut kind,
        None,
    )?;
    let early_execution_error = get_early_execution_error(
        &tx_digest,
        &checked_input_objects,
        context.certificate_deny_set,
        &FundsWithdrawStatus::MaybeSufficient,
    );
    let execution_params = match early_execution_error {
        Some(error) => ExecutionOrEarlyError::Err(error),
        None => ExecutionOrEarlyError::Ok(()),
    };

    let tracking_store = TrackingBackingStore::new(context.backing_store);

    // Clone inputs for potential retry if object funds check fails post-execution.
    let cloned_input_objects = checked_input_objects.clone();
    let cloned_gas = gas_data.clone();
    let cloned_kind = kind.clone();
    let (inner_temp_store, _, effects, execution_result) = executor.dev_inspect_transaction(
        &tracking_store,
        context.protocol_config,
        context.execution_metrics.clone(),
        false, // expensive_checks
        execution_params,
        &context.epoch_id,
        context.epoch_timestamp_ms,
        checked_input_objects,
        gas_data,
        gas_status,
        kind,
        rewritten_inputs.clone(),
        signer,
        tx_digest,
        dev_inspect,
    );

    // Post-execution: check object funds (non-address withdrawals discovered during execution).
    let (inner_temp_store, effects, execution_result) = if execution_result.is_ok() {
        let has_insufficient_object_funds = inner_temp_store
            .accumulator_running_max_withdraws
            .iter()
            .filter(|(id, _)| !address_funds.contains(id))
            .any(|(id, max_withdraw)| {
                let (balance, _) = context.account_funds_read.get_latest_account_amount(id);
                balance < *max_withdraw
            });

        if has_insufficient_object_funds {
            let retry_gas_status = SuiGasStatus::new(
                cloned_gas.budget,
                cloned_gas.price,
                context.reference_gas_price,
                context.protocol_config,
            )?;
            let (store, _, effects, result) = executor.dev_inspect_transaction(
                &tracking_store,
                context.protocol_config,
                context.execution_metrics.clone(),
                false,
                ExecutionOrEarlyError::Err(ExecutionErrorKind::InsufficientFundsForWithdraw),
                &context.epoch_id,
                context.epoch_timestamp_ms,
                cloned_input_objects,
                cloned_gas,
                retry_gas_status,
                cloned_kind,
                rewritten_inputs,
                signer,
                tx_digest,
                dev_inspect,
            );
            (store, effects, result)
        } else {
            (inner_temp_store, effects, execution_result)
        }
    } else {
        (inner_temp_store, effects, execution_result)
    };

    let loaded_runtime_objects = tracking_store.into_read_objects();
    let unchanged_loaded_runtime_objects =
        crate::transaction_outputs::unchanged_loaded_runtime_objects(
            &transaction,
            &effects,
            &loaded_runtime_objects,
        );

    let object_set = {
        let objects = {
            let mut objects = loaded_runtime_objects;

            for object in inner_temp_store
                .input_objects
                .into_values()
                .chain(inner_temp_store.written.into_values())
            {
                objects.insert(object);
            }

            objects
        };

        let object_keys = sui_types::storage::get_transaction_object_set(
            &transaction,
            &effects,
            &unchanged_loaded_runtime_objects,
        );

        let mut set = sui_types::full_checkpoint_content::ObjectSet::default();
        for key in object_keys {
            if let Some(object) = objects.get(&key) {
                set.insert(object.clone());
            }
        }

        set
    };

    Ok(SimulateTransactionResult {
        objects: object_set,
        events: effects.events_digest().map(|_| inner_temp_store.events),
        effects,
        execution_result,
        mock_gas_id,
        unchanged_loaded_runtime_objects,
        suggested_gas_price: reader.suggested_gas_price(&transaction),
    })
}
