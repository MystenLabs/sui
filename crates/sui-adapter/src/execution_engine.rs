// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::execution_mode::{self, ExecutionMode};
use move_binary_format::access::ModuleAccess;
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_vm_runtime::move_vm::MoveVM;
use sui_types::balance::{
    BALANCE_CREATE_REWARDS_FUNCTION_NAME, BALANCE_DESTROY_REBATES_FUNCTION_NAME,
    BALANCE_MODULE_NAME,
};
use sui_types::base_types::ObjectID;
use sui_types::gas_coin::GAS;
use sui_types::metrics::LimitsMetrics;
use sui_types::object::OBJECT_START_VERSION;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use tracing::{info, instrument, trace, warn};

use crate::programmable_transactions;
use sui_macros::checked_arithmetic;
use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
use sui_types::clock::{CLOCK_MODULE_NAME, CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME};
use sui_types::committee::EpochId;
use sui_types::effects::TransactionEffects;
use sui_types::epoch_data::EpochData;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::execution_status::ExecutionStatus;
use sui_types::gas::{GasCostSummary, SuiGasStatusAPI};
use sui_types::messages::{
    Argument, Command, ConsensusCommitPrologue, GenesisTransaction, ObjectArg,
    ProgrammableTransaction, TransactionKind,
};
use sui_types::storage::{ChildObjectResolver, ObjectStore, ParentSync, WriteKind};
#[cfg(msim)]
use sui_types::sui_system_state::advance_epoch_result_injection::maybe_modify_result;
use sui_types::sui_system_state::{AdvanceEpochParams, ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME};
use sui_types::temporary_store::InnerTemporaryStore;
use sui_types::temporary_store::TemporaryStore;
use sui_types::{
    base_types::{ObjectRef, SuiAddress, TransactionDigest, TxContext},
    gas::SuiGasStatus,
    messages::{CallArg, ChangeEpoch},
    object::Object,
    storage::BackingPackageStore,
    sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{
    is_system_package, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_FRAMEWORK_OBJECT_ID, SUI_SYSTEM_OBJECT_ID, SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

checked_arithmetic! {

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver + ObjectStore + GetModule,
>(
    shared_object_refs: Vec<ObjectRef>,
    temporary_store: TemporaryStore<S>,
    transaction_kind: TransactionKind,
    transaction_signer: SuiAddress,
    gas: &[ObjectRef],
    transaction_digest: TransactionDigest,
    transaction_dependencies: BTreeSet<TransactionDigest>,
    move_vm: &Arc<MoveVM>,
    gas_status: SuiGasStatus,
    epoch_data: &EpochData,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    enable_expensive_checks: bool
) -> (
    InnerTemporaryStore,
    TransactionEffects,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    // Separating out impl so we can call this from other context
    execute_transaction_to_effects_impl::<Mode, S>(
        shared_object_refs,
        temporary_store,
        transaction_kind,
        transaction_signer,
        gas,
        transaction_digest,
        transaction_dependencies,
        move_vm,
        gas_status,
        &epoch_data.epoch_id(),
        epoch_data.epoch_start_timestamp(),
        protocol_config,
        metrics,
        enable_expensive_checks
    )
}

/// Separating out impl so we can call this from other context
pub fn execute_transaction_to_effects_impl<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver + ObjectStore + GetModule,
>(
    shared_object_refs: Vec<ObjectRef>,
    mut temporary_store: TemporaryStore<S>,
    transaction_kind: TransactionKind,
    transaction_signer: SuiAddress,
    gas: &[ObjectRef],
    transaction_digest: TransactionDigest,
    mut transaction_dependencies: BTreeSet<TransactionDigest>,
    move_vm: &Arc<MoveVM>,
    gas_status: SuiGasStatus,
    epoch_id: &EpochId,
    epoch_timestamp_ms: u64,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    enable_expensive_checks: bool
) -> (
    InnerTemporaryStore,
    TransactionEffects,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    let mut tx_ctx = TxContext::new_from_components(&transaction_signer, &transaction_digest, epoch_id, epoch_timestamp_ms);

    let is_epoch_change = matches!(transaction_kind, TransactionKind::ChangeEpoch(_));

    let (gas_cost_summary, execution_result) = execute_transaction::<Mode, _>(
        &mut temporary_store,
        transaction_kind,
        gas,
        &mut tx_ctx,
        move_vm,
        gas_status,
        protocol_config,
        metrics,
        enable_expensive_checks
    );

    let status = if let Err(error) = &execution_result {
        // Elaborate errors in logs if they are unexpected or their status is terse.
        use ExecutionErrorKind as K;
        match error.kind() {
            K::InvariantViolation |
            K::VMInvariantViolation => {
                #[skip_checked_arithmetic]
                tracing::error!(
                    kind = ?error.kind(),
                    tx_digest = ?transaction_digest,
                    "INVARIANT VIOLATION! Source: {:?}",
                    error.source(),
                );
            },

            K::SuiMoveVerificationError |
            K::VMVerificationOrDeserializationError => {
                #[skip_checked_arithmetic]
                tracing::debug!(
                    kind = ?error.kind(),
                    tx_digest = ?transaction_digest,
                    "Verification Error. Source: {:?}",
                    error.source(),
                );
            }

            K::PublishUpgradeMissingDependency |
            K::PublishUpgradeDependencyDowngrade => {
                #[skip_checked_arithmetic]
                tracing::debug!(
                    kind = ?error.kind(),
                    tx_digest = ?transaction_digest,
                    "Publish/Upgrade Error. Source: {:?}",
                    error.source(),
                )
            }

            _ => (),
        };

        let (status, command) = error.to_execution_status();
        ExecutionStatus::new_failure(status, command)
    } else {
        ExecutionStatus::Success
    };

    #[skip_checked_arithmetic]
    trace!(
        tx_digest = ?transaction_digest,
        computation_gas_cost = gas_cost_summary.computation_cost,
        storage_gas_cost = gas_cost_summary.storage_cost,
        storage_gas_rebate = gas_cost_summary.storage_rebate,
        "Finished execution of transaction with status {:?}",
        status
    );

    // Remove from dependencies the generic hash
    transaction_dependencies.remove(&TransactionDigest::genesis());

    if enable_expensive_checks && !Mode::allow_arbitrary_function_calls() {
        temporary_store
            .check_ownership_invariants(&transaction_signer, gas, is_epoch_change)
            .unwrap()
    } // else, in dev inspect mode and anything goes--don't check

    let (inner, effects) = temporary_store.to_effects(
        shared_object_refs,
        &transaction_digest,
        transaction_dependencies.into_iter().collect(),
        gas_cost_summary,
        status,
        gas,
        *epoch_id,
    );
    (inner, effects, execution_result)
}

fn charge_gas_for_object_read<S>(
    temporary_store: &TemporaryStore<S>,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    // Charge gas for reading all objects from the DB.
    let total_size = temporary_store
        .objects()
        .iter()
        // don't charge for loading Sui Framework or Move stdlib
        .filter(|(id, _)| !is_system_package(**id))
        .map(|(_, obj)| obj.object_size_for_gas_metering())
        .sum();
    gas_status.charge_storage_read(total_size)
}

#[instrument(name = "tx_execute", level = "debug", skip_all)]
fn execute_transaction<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver + ObjectStore + GetModule,
>(
    temporary_store: &mut TemporaryStore<S>,
    transaction_kind: TransactionKind,
    gas: &[ObjectRef],
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    mut gas_status: SuiGasStatus,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    enable_expensive_checks: bool
) -> (
    GasCostSummary,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    // First smash gas into the first coin if more than 1 was provided
    let gas_object_ref = match temporary_store.smash_gas(gas) {
        Ok(obj_ref) => obj_ref,
        Err(_) => gas[0], // this cannot fail, but we use gas[0] anyway
    };
    // At this point no charge has been applied yet
    debug_assert!(
        gas_status.gas_used() == 0
            && gas_status.storage_rebate() == 0
            && gas_status.storage_gas_units() == 0,
        "No gas charges must be applied yet"
    );
    let is_genesis_tx = matches!(transaction_kind, TransactionKind::Genesis(_));
    let advance_epoch_gas_summary = transaction_kind.get_advance_epoch_tx_gas_summary();

    // We must charge object read here during transaction execution, because if this fails
    // we must still ensure an effect is committed and all objects versions incremented
    let result = charge_gas_for_object_read(temporary_store, &mut gas_status);
    let mut result = result.and_then(|()| {
        let mut execution_result = execution_loop::<Mode, _>(
            temporary_store,
            transaction_kind,
            gas_object_ref.0,
            tx_ctx,
            move_vm,
            &mut gas_status,
            protocol_config,
            metrics.clone(),
        );

        let effects_estimated_size = temporary_store.estimate_effects_size_upperbound();

        // Check if a limit threshold was crossed.
        // For metered transactions, there is not soft limit.
        // For system transactions, we allow a soft limit with alerting, and a hard limit where we terminate
        match check_limit_by_meter!(
            !gas_status.is_unmetered(),
            effects_estimated_size,
            protocol_config.max_serialized_tx_effects_size_bytes(),
            protocol_config.max_serialized_tx_effects_size_bytes_system_tx(),
            metrics.excessive_estimated_effects_size
        ) {
            LimitThresholdCrossed::None => (),
            LimitThresholdCrossed::Soft(_, limit) => {
                warn!(
                    effects_estimated_size = effects_estimated_size,
                    soft_limit = limit,
                    "Estimated transaction effects size crossed soft limit",
                )
            }
            LimitThresholdCrossed::Hard(_, lim) => {
                execution_result = Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::EffectsTooLarge {
                        current_size: effects_estimated_size as u64,
                        max_size: lim as u64,
                    },
                    "Transaction effects are too large",
                ))
            }
        };
        if execution_result.is_ok() {

            // This limit is only present in Version 3 and up, so use this to gate it
            if let (Some(normal_lim), Some(system_lim)) =
                (protocol_config.max_size_written_objects_as_option(), protocol_config
            .max_size_written_objects_system_tx_as_option()) {
                let written_objects_size = temporary_store.written_objects_size();

                match check_limit_by_meter!(
                    !gas_status.is_unmetered(),
                    written_objects_size,
                    normal_lim,
                    system_lim,
                    metrics.excessive_written_objects_size
                ) {
                    LimitThresholdCrossed::None => (),
                    LimitThresholdCrossed::Soft(_, limit) => {
                        warn!(
                            written_objects_size = written_objects_size,
                            soft_limit = limit,
                            "Written objects size crossed soft limit",
                        )
                    }
                    LimitThresholdCrossed::Hard(_, lim) => {
                        execution_result = Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::WrittenObjectsTooLarge {
                                current_size: written_objects_size as u64,
                                max_size: lim as u64,
                            },
                            "Written objects size crossed hard limit",
                        ))
                    }
                };

            }

        }

        execution_result
    });

    if protocol_config.gas_model_version() > 1 {
        // We always go through the gas charging process, but for system transaction, we don't pass
        // the gas object ID since it's not a valid object.
        // TODO: Ideally we should make gas object ref None in the first place.
        let gas_object_id = if gas_status.is_unmetered() {
            None
        } else {
            Some(gas_object_ref.0)
        };
        let cost_summary =
            temporary_store.charge_gas(gas_object_id, &mut gas_status, &mut result, gas);
        // === begin SUI conservation checks ===
        // For advance epoch transaction, we need to provide epoch rewards and rebates as extra
        // information provided to check_sui_conserved, because we mint rewards, and burn
        // the rebates. We also need to pass in the unmetered_storage_rebate because storage
        // rebate is not reflected in the storage_rebate of gas summary. This is a bit confusing.
        // We could probably clean up the code a bit.
        // Put all the storage rebate accumulated in the system transaction
        // to the 0x5 object so that it's not lost.
        temporary_store.conserve_unmetered_storage_rebate(gas_status.unmetered_storage_rebate());
        if !is_genesis_tx && !Mode::allow_arbitrary_values() {
            // ensure that this transaction did not create or destroy SUI, try to recover if the check fails
            let conservation_result = temporary_store.check_sui_conserved(advance_epoch_gas_summary, enable_expensive_checks);
            if let Err(conservation_err) = conservation_result {
                // conservation violated. try to avoid panic by dumping all writes, charging for gas, re-checking
                // conservation, and surfacing an aborted transaction with an invariant violation if all of that works
                result = Err(conservation_err);
                temporary_store.reset(gas, &mut gas_status);
                temporary_store.charge_gas(gas_object_id, &mut gas_status, &mut result, gas);
                // check conservation once more more
                if let Err(recovery_err) = temporary_store.check_sui_conserved(advance_epoch_gas_summary, enable_expensive_checks) {
                    // if we still fail, it's a problem with gas
                    // charging that happens even in the "aborted" case--no other option but panic.
                    // we will create or destroy SUI otherwise
                    panic!("SUI conservation fail in tx block {}: {}\nGas status is {}\nTx was ", tx_ctx.digest(), recovery_err, gas_status.summary())
                }
              }
        } // else, we're in the genesis transaction which mints the SUI supply, and hence does not satisfy SUI conservation, or
        // we're in the non-production dev inspect mode which allows us to violate conservation
        // === end SUI conservation checks ===
        (cost_summary, result)
    } else {
        // legacy code before gas v2, leave it alone
        if !gas_status.is_unmetered() {
            temporary_store.charge_gas_legacy(gas_object_ref.0, &mut gas_status, &mut result, gas);
        }
        let cost_summary = gas_status.summary();
        (cost_summary, result)
    }
}

fn execution_loop<
    Mode: ExecutionMode,
    S: BackingPackageStore + ParentSync + ChildObjectResolver + ObjectStore + GetModule,
>(
    temporary_store: &mut TemporaryStore<S>,
    transaction_kind: TransactionKind,
    gas_object_id: ObjectID,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> Result<Mode::ExecutionResults, ExecutionError> {
    match transaction_kind {
        TransactionKind::ChangeEpoch(change_epoch) => {
            advance_epoch(
                change_epoch,
                temporary_store,
                tx_ctx,
                move_vm,
                gas_status,
                protocol_config,
                metrics,
            )?;
            Ok(Mode::empty_results())
        }
        TransactionKind::Genesis(GenesisTransaction { objects }) => {
            if tx_ctx.epoch() != 0 {
                panic!("BUG: Genesis Transactions can only be executed in epoch 0");
            }

            for genesis_object in objects {
                match genesis_object {
                    sui_types::messages::GenesisObject::RawObject { data, owner } => {
                        let object = Object {
                            data,
                            owner,
                            previous_transaction: tx_ctx.digest(),
                            storage_rebate: 0,
                        };
                        temporary_store.write_object(object, WriteKind::Create);
                    }
                }
            }
            Ok(Mode::empty_results())
        }
        TransactionKind::ConsensusCommitPrologue(prologue) => {
            setup_consensus_commit(
                prologue,
                temporary_store,
                tx_ctx,
                move_vm,
                gas_status,
                protocol_config,
                metrics,
            ).expect("ConsensusCommitPrologue cannot fail");
            Ok(Mode::empty_results())
        }
        TransactionKind::ProgrammableTransaction(pt) => {
            programmable_transactions::execution::execute::<_, Mode>(
                protocol_config,
                metrics,
                move_vm,
                temporary_store,
                tx_ctx,
                gas_status,
                Some(gas_object_id),
                pt,
            )
        }
    }
}

fn mint_epoch_rewards_in_pt(
    builder: &mut ProgrammableTransactionBuilder,
    params: &AdvanceEpochParams,
) -> (Argument, Argument) {
    // Create storage rewards.
    let storage_charge_arg = builder
        .input(CallArg::Pure(
            bcs::to_bytes(&params.storage_charge).unwrap(),
        ))
        .unwrap();
    let storage_rewards = builder.programmable_move_call(
        SUI_FRAMEWORK_OBJECT_ID,
        BALANCE_MODULE_NAME.to_owned(),
        BALANCE_CREATE_REWARDS_FUNCTION_NAME.to_owned(),
        vec![GAS::type_tag()],
        vec![storage_charge_arg],
    );

    // Create computation rewards.
    let computation_charge_arg = builder
        .input(CallArg::Pure(
            bcs::to_bytes(&params.computation_charge).unwrap(),
        ))
        .unwrap();
    let computation_rewards = builder.programmable_move_call(
        SUI_FRAMEWORK_OBJECT_ID,
        BALANCE_MODULE_NAME.to_owned(),
        BALANCE_CREATE_REWARDS_FUNCTION_NAME.to_owned(),
        vec![GAS::type_tag()],
        vec![computation_charge_arg],
    );
    (storage_rewards, computation_rewards)
}

pub fn construct_advance_epoch_pt(
    params: &AdvanceEpochParams,
) -> Result<ProgrammableTransaction, ExecutionError> {
    let mut builder = ProgrammableTransactionBuilder::new();
    // Step 1: Create storage and computation rewards.
    let (storage_rewards, computation_rewards) = mint_epoch_rewards_in_pt(&mut builder, params);

    // Step 2: Advance the epoch.
    let mut arguments = vec![storage_rewards, computation_rewards];
    let system_object_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });
    let call_arg_arguments = vec![
        system_object_arg,
        CallArg::Pure(bcs::to_bytes(&params.epoch).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.next_protocol_version.as_u64()).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.storage_rebate).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.non_refundable_storage_fee).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.storage_fund_reinvest_rate).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.reward_slashing_rate).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.epoch_start_timestamp_ms).unwrap()),
    ]
    .into_iter()
    .map(|a| builder.input(a))
    .collect::<Result<_, _>>();

    assert_invariant!(
        call_arg_arguments.is_ok(),
        "Unable to generate args for advance_epoch transaction!"
    );

    arguments.append(&mut call_arg_arguments.unwrap());

    info!(
        "Call arguments to advance_epoch transaction: {:?}",
        params
    );

    let storage_rebates = builder.programmable_move_call(
        SUI_SYSTEM_OBJECT_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADVANCE_EPOCH_FUNCTION_NAME.to_owned(),
        vec![],
        arguments,
    );

    // Step 3: Destroy the storage rebates.
    builder.programmable_move_call(
        SUI_FRAMEWORK_OBJECT_ID,
        BALANCE_MODULE_NAME.to_owned(),
        BALANCE_DESTROY_REBATES_FUNCTION_NAME.to_owned(),
        vec![GAS::type_tag()],
        vec![storage_rebates],
    );
    Ok(builder.finish())
}

pub fn construct_advance_epoch_safe_mode_pt(
    params: &AdvanceEpochParams,
    protocol_config: &ProtocolConfig,
) -> Result<ProgrammableTransaction, ExecutionError> {
    let mut builder = ProgrammableTransactionBuilder::new();
    // Step 1: Create storage and computation rewards.
    let (storage_rewards, computation_rewards) = mint_epoch_rewards_in_pt(&mut builder, params);

    // Step 2: Advance the epoch.
    let mut arguments = vec![storage_rewards, computation_rewards];
    let system_object_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });

    let mut args = vec![
        system_object_arg,
        CallArg::Pure(bcs::to_bytes(&params.epoch).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.next_protocol_version.as_u64()).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.storage_rebate).unwrap()),
        CallArg::Pure(bcs::to_bytes(&params.non_refundable_storage_fee).unwrap()),
    ];

    if protocol_config.get_advance_epoch_start_time_in_safe_mode() {
        args.push(CallArg::Pure(
            bcs::to_bytes(&params.epoch_start_timestamp_ms).unwrap(),
        ));
    }

    let call_arg_arguments = args
        .into_iter()
        .map(|a| builder.input(a))
        .collect::<Result<_, _>>();

    assert_invariant!(
        call_arg_arguments.is_ok(),
        "Unable to generate args for advance_epoch transaction!"
    );

    arguments.append(&mut call_arg_arguments.unwrap());

    info!(
        "Call arguments to advance_epoch transaction: {:?}",
        params
    );

    builder.programmable_move_call(
        SUI_SYSTEM_OBJECT_ID,
        SUI_SYSTEM_MODULE_NAME.to_owned(),
        ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME.to_owned(),
        vec![],
        arguments,
    );

    Ok(builder.finish())
}

fn advance_epoch<S: ObjectStore + BackingPackageStore + ParentSync + ChildObjectResolver>(
    change_epoch: ChangeEpoch,
    temporary_store: &mut TemporaryStore<S>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> Result<(), ExecutionError> {
    let params = AdvanceEpochParams {
        epoch: change_epoch.epoch,
        next_protocol_version: change_epoch.protocol_version,
        storage_charge: change_epoch.storage_charge,
        computation_charge: change_epoch.computation_charge,
        storage_rebate: change_epoch.storage_rebate,
        non_refundable_storage_fee: change_epoch.non_refundable_storage_fee,
        storage_fund_reinvest_rate: protocol_config.storage_fund_reinvest_rate(),
        reward_slashing_rate: protocol_config.reward_slashing_rate(),
        epoch_start_timestamp_ms: change_epoch.epoch_start_timestamp_ms,
    };
    let advance_epoch_pt = construct_advance_epoch_pt(&params)?;
    let result = programmable_transactions::execution::execute::<_, execution_mode::System>(
        protocol_config,
        metrics.clone(),
        move_vm,
        temporary_store,
        tx_ctx,
        gas_status,
        None,
        advance_epoch_pt,
    );

    #[cfg(msim)]
    let result = maybe_modify_result(result, change_epoch.epoch);

    if result.is_err() {
        tracing::error!(
            "Failed to execute advance epoch transaction. Switching to safe mode. Error: {:?}. Input objects: {:?}. Tx data: {:?}",
            result.as_ref().err(),
            temporary_store.objects(),
            change_epoch,
        );
        temporary_store.drop_writes();
        // Must reset the storage rebate since we are re-executing.
        gas_status.reset_storage_cost_and_rebate();

        if protocol_config.get_advance_epoch_start_time_in_safe_mode() {
            temporary_store.advance_epoch_safe_mode(&params, protocol_config);
        } else {
            let advance_epoch_safe_mode_pt =
                construct_advance_epoch_safe_mode_pt(&params, protocol_config)?;
            programmable_transactions::execution::execute::<_, execution_mode::System>(
                protocol_config,
                metrics.clone(),
                move_vm,
                temporary_store,
                tx_ctx,
                gas_status,
                None,
                advance_epoch_safe_mode_pt,
            )
            .expect("Advance epoch with safe mode must succeed");
        }
    }

    for (version, modules, dependencies) in change_epoch.system_packages.into_iter() {
        let max_format_version = protocol_config.move_binary_format_version();
        let deserialized_modules: Vec<_> = modules
            .iter()
            .map(|m| CompiledModule::deserialize_with_config(m, max_format_version, protocol_config.no_extraneous_module_bytes()).unwrap())
            .collect();

        if version == OBJECT_START_VERSION {
            let package_id = deserialized_modules.first().unwrap().address();
            info!("adding new system package {package_id}");

            let publish_pt = {
                let mut b = ProgrammableTransactionBuilder::new();
                b.command(Command::Publish(modules, dependencies));
                b.finish()
            };

            programmable_transactions::execution::execute::<_, execution_mode::System>(
                protocol_config,
                metrics.clone(),
                move_vm,
                temporary_store,
                tx_ctx,
                gas_status,
                None,
                publish_pt,
            )
            .expect("System Package Publish must succeed");
        } else {
            let mut new_package = Object::new_system_package(
                &deserialized_modules, version, dependencies, tx_ctx.digest(),
            );

            info!(
                "upgraded system package {:?}",
                new_package.compute_object_reference()
            );

            // Decrement the version before writing the package so that the store can record the
            // version growing by one in the effects.
            new_package
                .data
                .try_as_package_mut()
                .unwrap()
                .decrement_version();

            // upgrade of a previously existing framework module
            temporary_store.write_object(new_package, WriteKind::Mutate);
        }
    }

    Ok(())
}

/// Perform metadata updates in preparation for the transactions in the upcoming checkpoint:
///
/// - Set the timestamp for the `Clock` shared object from the timestamp in the header from
///   consensus.
fn setup_consensus_commit<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    prologue: ConsensusCommitPrologue,
    temporary_store: &mut TemporaryStore<S>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
) -> Result<(), ExecutionError> {
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let res = builder.move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            CLOCK_MODULE_NAME.to_owned(),
            CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME.to_owned(),
            vec![],
            vec![
                CallArg::Object(ObjectArg::SharedObject {
                    id: SUI_CLOCK_OBJECT_ID,
                    initial_shared_version: SUI_CLOCK_OBJECT_SHARED_VERSION,
                    mutable: true,
                }),
                CallArg::Pure(bcs::to_bytes(&prologue.commit_timestamp_ms).unwrap()),
            ],
        );
        assert_invariant!(
            res.is_ok(),
            "Unable to generate consensus_commit_prologue transaction!"
        );
        builder.finish()
    };
    programmable_transactions::execution::execute::<_, execution_mode::System>(
        protocol_config,
        metrics,
        move_vm,
        temporary_store,
        tx_ctx,
        gas_status,
        None,
        pt,
    )
}

}
