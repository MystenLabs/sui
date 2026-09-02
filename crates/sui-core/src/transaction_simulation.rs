// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeSet, HashSet},
    sync::Arc,
};

use nonempty::NonEmpty;
use sui_config::{
    transaction_deny_config::TransactionDenyConfig, verifier_signing_config::VerifierSigningConfig,
};
use sui_execution::Executor;
use sui_transaction_checks::{check_dev_inspect_input, check_transaction_input};
use sui_types::{
    base_types::{EpochId, ObjectID, ObjectRef, SystemObjectVersions},
    coin_reservation::{CoinReservationResolverTrait, ParsedDigest},
    digests::{ChainIdentifier, TransactionDigest},
    effects::TransactionEffectsAPI,
    error::{SuiErrorKind, SuiResult},
    execution_params::{ExecutionOrEarlyError, FundsWithdrawStatus, get_early_execution_error},
    execution_status::ExecutionErrorKind,
    full_checkpoint_content::ObjectSet,
    gas::SuiGasStatus,
    messages_checkpoint::CheckpointTimestamp,
    metrics::{BytecodeVerifierMetrics, ExecutionMetrics},
    object::{MoveObject, OBJECT_START_VERSION, Object, Owner},
    storage::{
        BackingPackageStore, BackingStore, TrackingBackingStore, get_transaction_object_set,
    },
    transaction::{
        InputObjectKind, InputObjects, ObjectReadResult, ReceivingObjects, TransactionData,
        TransactionDataAPI, TxValidityCheckContext,
    },
    transaction_executor::{SimulateTransactionResult, TransactionChecks},
};

use crate::{
    accumulators::{
        funds_read::AccountFundsRead,
        transaction_rewriting::rewrite_transaction_for_coin_reservations,
    },
    authority::{DEV_INSPECT_GAS_COIN_VALUE, pre_object_load_checks},
    transaction_outputs::unchanged_loaded_runtime_objects,
};

/// Load transaction inputs for simulation without preparing them for committed execution.
pub trait SimulationInputLoader {
    /// Load the input and receiving objects at the state used for simulation.
    fn read_objects_for_simulation(
        &self,
        transaction_digest: &TransactionDigest,
        input_object_kinds: &[InputObjectKind],
        receiving_object_refs: &[ObjectRef],
        epoch_id: EpochId,
    ) -> SuiResult<(InputObjects, ReceivingObjects)>;
}

/// Simulate a transaction without committing its outputs.
pub fn simulate_transaction(
    mut transaction: TransactionData,
    checks: TransactionChecks,
    allow_mock_gas_coin: bool,
    suggested_gas_price: Option<u64>,
    validity_check_context: TxValidityCheckContext<'_>,
    execution_epoch_id: EpochId,
    epoch_timestamp_ms: CheckpointTimestamp,
    chain_identifier: ChainIdentifier,
    transaction_deny_config: &TransactionDenyConfig,
    certificate_deny_set: &HashSet<TransactionDigest>,
    input_loader: &dyn SimulationInputLoader,
    backing_store: &(dyn BackingStore + Send + Sync),
    backing_package_store: &(dyn BackingPackageStore + Send + Sync),
    executor: &(dyn Executor + Send + Sync),
    coin_reservation_resolver: &dyn CoinReservationResolverTrait,
    account_funds_read: &dyn AccountFundsRead,
    verifier_signing_config: &VerifierSigningConfig,
    bytecode_verifier_metrics: &Arc<BytecodeVerifierMetrics>,
    execution_metrics: &Arc<ExecutionMetrics>,
) -> SuiResult<SimulateTransactionResult> {
    let dev_inspect = checks.disabled();

    // Reject coin reservations in gas payment when the execution engine
    // doesn't support them.
    let protocol_config = validity_check_context.config;
    if !protocol_config.enable_coin_reservation_obj_refs()
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

    // Compute input/receiving object kinds before mock gas injection so the mock
    // gas reference is not included in input_object_kinds (it is added to
    // input_objects directly after object loading).
    let input_object_kinds = transaction.input_objects()?;
    let receiving_object_refs = transaction.receiving_objects();

    // Inject mock gas coin before validity_check so that on protocol versions
    // where address-balance gas payments are not yet enabled, the non-empty
    // payment check in validity_check passes for simulate/dev-inspect requests
    // submitted without explicit gas.
    // Also required before pre_object_load_checks so that funds-withdrawal
    // processing sees non-empty payment and doesn't create an address-balance
    // withdrawal for gas.
    // Skip mock gas for gasless transactions — they don't use gas coins.
    let is_gasless = protocol_config.enable_gasless() && transaction.is_gasless_transaction();
    let mock_gas_object = if allow_mock_gas_coin && transaction.gas().is_empty() && !is_gasless {
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

    // Full validity check including gas budget and price.
    transaction.validity_check(&validity_check_context)?;

    let declared_withdrawals = pre_object_load_checks(
        &transaction,
        &[],
        &input_object_kinds,
        &receiving_object_refs,
        protocol_config,
        transaction_deny_config,
        backing_package_store,
        chain_identifier,
        coin_reservation_resolver,
        account_funds_read,
    )?;
    let address_funds: BTreeSet<_> = declared_withdrawals.keys().cloned().collect();

    let transaction_digest = transaction.digest();
    let (mut input_objects, receiving_objects) = input_loader.read_objects_for_simulation(
        &transaction_digest,
        &input_object_kinds,
        &receiving_object_refs,
        validity_check_context.epoch,
    )?;

    // Add mock gas to input objects after loading (it doesn't exist in the store).
    let mock_gas_id = mock_gas_object.map(|obj| {
        let id = obj.id();
        input_objects.push(ObjectReadResult::new_from_gas_object(&obj));
        id
    });

    let (gas_status, checked_input_objects) = if dev_inspect {
        check_dev_inspect_input(
            protocol_config,
            &transaction,
            input_objects,
            receiving_objects,
            validity_check_context.reference_gas_price,
        )?
    } else {
        check_transaction_input(
            protocol_config,
            validity_check_context.reference_gas_price,
            &transaction,
            input_objects,
            &receiving_objects,
            bytecode_verifier_metrics,
            verifier_signing_config,
        )?
    };

    let (mut kind, signer, gas_data) = transaction.execution_parts();
    let rewritten_inputs = rewrite_transaction_for_coin_reservations(
        chain_identifier,
        coin_reservation_resolver,
        signer,
        &mut kind,
        None,
    )?;
    let early_execution_error = get_early_execution_error(
        &transaction.digest(),
        &checked_input_objects,
        certificate_deny_set,
        &FundsWithdrawStatus::MaybeSufficient,
    );
    // Dev-inspect/simulation path (not committed): no assigned accumulator version here, so the
    // IFFW short-circuit applies unconditionally (`None`), matching non-mainnet execution.
    let execution_params = match early_execution_error {
        None => ExecutionOrEarlyError::ok(None),
        Some(errors) => ExecutionOrEarlyError::failed(errors, None),
    };

    let tracking_store = TrackingBackingStore::new(backing_store);

    // Clone inputs for potential retry if object funds check fails post-execution.
    let cloned_input_objects = checked_input_objects.clone();
    let cloned_gas = gas_data.clone();
    let cloned_kind = kind.clone();
    let tx_digest = transaction_digest;
    let system_object_versions = SystemObjectVersions::from_latest_in_store(backing_store);
    let (inner_temp_store, _, effects, execution_result) = executor.dev_inspect_transaction(
        &tracking_store,
        protocol_config,
        execution_metrics.clone(),
        false, // expensive_checks
        execution_params,
        &execution_epoch_id,
        epoch_timestamp_ms,
        checked_input_objects,
        system_object_versions,
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
                let balance = account_funds_read.get_latest_account_amount(id);
                balance < *max_withdraw
            });

        if has_insufficient_object_funds {
            let retry_gas_status = SuiGasStatus::new(
                cloned_gas.budget,
                cloned_gas.price,
                validity_check_context.reference_gas_price,
                protocol_config,
            )?;
            let (store, _, effects, result) = executor.dev_inspect_transaction(
                &tracking_store,
                protocol_config,
                execution_metrics.clone(),
                false,
                ExecutionOrEarlyError::failed(
                    NonEmpty::new(ExecutionErrorKind::InsufficientFundsForWithdraw),
                    None,
                ),
                &execution_epoch_id,
                epoch_timestamp_ms,
                cloned_input_objects,
                system_object_versions,
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
        unchanged_loaded_runtime_objects(&transaction, &effects, &loaded_runtime_objects);

    let object_set = {
        let objects = {
            let mut objects = loaded_runtime_objects;

            for o in inner_temp_store
                .input_objects
                .into_values()
                .chain(inner_temp_store.written.into_values())
            {
                objects.insert(o);
            }

            objects
        };

        let object_keys =
            get_transaction_object_set(&transaction, &effects, &unchanged_loaded_runtime_objects);

        let mut set = ObjectSet::default();
        for k in object_keys {
            if let Some(o) = objects.get(&k) {
                set.insert(o.clone());
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
        suggested_gas_price,
    })
}
