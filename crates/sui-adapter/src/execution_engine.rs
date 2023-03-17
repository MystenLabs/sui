// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{collections::BTreeSet, sync::Arc};

use crate::execution_mode::{self, ExecutionMode};
use move_binary_format::CompiledModule;
use move_bytecode_utils::module_cache::GetModule;
use move_core_types::language_storage::ModuleId;
use move_vm_runtime::move_vm::MoveVM;
use sui_types::base_types::ObjectID;
use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
use tracing::{debug, info, instrument, warn};

use crate::programmable_transactions;
use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
use sui_types::epoch_data::EpochData;
use sui_types::error::{ExecutionError, ExecutionErrorKind};
use sui_types::gas::GasCostSummary;
use sui_types::messages::{
    ConsensusCommitPrologue, GenesisTransaction, ObjectArg, TransactionKind,
};
use sui_types::storage::{ChildObjectResolver, ObjectStore, ParentSync, WriteKind};
use sui_types::sui_system_state::{
    get_sui_system_state_version, ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME,
    CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME,
};
use sui_types::temporary_store::InnerTemporaryStore;
use sui_types::{
    base_types::{ObjectRef, SuiAddress, TransactionDigest, TxContext},
    gas::SuiGasStatus,
    messages::{CallArg, ChangeEpoch, ExecutionStatus, TransactionEffects},
    object::Object,
    storage::BackingPackageStore,
    sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
    SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_types::{
    is_system_package, SUI_CLOCK_OBJECT_ID, SUI_CLOCK_OBJECT_SHARED_VERSION,
    SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
};

use sui_types::temporary_store::TemporaryStore;

#[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
pub fn execute_transaction_to_effects<
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
    epoch_data: &EpochData,
    protocol_config: &ProtocolConfig,
) -> (
    InnerTemporaryStore,
    TransactionEffects,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    let mut tx_ctx = TxContext::new(&transaction_signer, &transaction_digest, epoch_data);

    let (gas_cost_summary, execution_result) = execute_transaction::<Mode, _>(
        &mut temporary_store,
        transaction_kind,
        gas,
        &mut tx_ctx,
        move_vm,
        gas_status,
        protocol_config,
    );

    let (status, execution_result) = match execution_result {
        Ok(results) => (ExecutionStatus::Success, Ok(results)),
        Err(error) => {
            let (status, command) = error.to_execution_status();
            (ExecutionStatus::new_failure(status, command), Err(error))
        }
    };
    debug!(
        computation_gas_cost = gas_cost_summary.computation_cost,
        storage_gas_cost = gas_cost_summary.storage_cost,
        storage_gas_rebate = gas_cost_summary.storage_rebate,
        "Finished execution of transaction with status {:?}",
        status
    );

    // Remove from dependencies the generic hash
    transaction_dependencies.remove(&TransactionDigest::genesis());

    let (inner, effects) = temporary_store.to_effects(
        shared_object_refs,
        &transaction_digest,
        transaction_dependencies.into_iter().collect(),
        gas_cost_summary,
        status,
        gas,
        epoch_data.epoch_id(),
    );
    (inner, effects, execution_result)
}

fn charge_gas_for_object_read<S>(
    temporary_store: &TemporaryStore<S>,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    // Charge gas for reading all objects from the DB.
    // TODO: Some of the objects may be duplicate (for batch tx). We could save gas by
    // fetching only unique objects.
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
) -> (
    GasCostSummary,
    Result<Mode::ExecutionResults, ExecutionError>,
) {
    // First smash gas into the first coin if more than 1 was provided
    let gas_object_ref = match temporary_store.smash_gas(gas) {
        Ok(obj_ref) => obj_ref,
        Err(_) => gas[0], // this cannot fail, but we use gas[0] anyway
    };
    let is_system = transaction_kind.is_system_tx();
    // We must charge object read gas inside here during transaction execution, because if this fails
    // we must still ensure an effect is committed and all objects versions incremented.
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
        );

        let effects_estimated_size = temporary_store.estimate_effects_size_upperbound();

        // Check if a limit threshold was crossed.
        // For metered transactions, there is not soft limit.
        // For system transactions, we allow a soft limit with alerting, and a hard limit where we terminate
        match check_limit_by_meter!(
            !gas_status.is_unmetered(),
            effects_estimated_size,
            protocol_config.max_serialized_tx_effects_size_bytes(),
            protocol_config.max_serialized_tx_effects_size_bytes_system_tx()
        ) {
            LimitThresholdCrossed::None => (),
            LimitThresholdCrossed::Soft(_, limit) => {
                /* TODO: add more alerting */
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
        execution_result
    });
    if !gas_status.is_unmetered() {
        temporary_store.charge_gas(gas_object_ref.0, &mut gas_status, &mut result, gas);
    }
    if !is_system {
        #[cfg(debug_assertions)]
        {
            // ensure that this transaction did not create or destroy SUI
            temporary_store.check_sui_conserved();
        }
    }
    let cost_summary = gas_status.summary();
    (cost_summary, result)
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
            )?;
            Ok(Mode::empty_results())
        }
        TransactionKind::ProgrammableTransaction(pt) => {
            programmable_transactions::execution::execute::<_, _, Mode>(
                protocol_config,
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

fn advance_epoch<S: BackingPackageStore + ParentSync + ChildObjectResolver>(
    change_epoch: ChangeEpoch,
    temporary_store: &mut TemporaryStore<S>,
    tx_ctx: &mut TxContext,
    move_vm: &Arc<MoveVM>,
    gas_status: &mut SuiGasStatus,
    protocol_config: &ProtocolConfig,
) -> Result<(), ExecutionError> {
    let module_id = ModuleId::new(SUI_FRAMEWORK_ADDRESS, SUI_SYSTEM_MODULE_NAME.to_owned());
    let function = ADVANCE_EPOCH_FUNCTION_NAME.to_owned();
    let system_object_arg = CallArg::Object(ObjectArg::SharedObject {
        id: SUI_SYSTEM_STATE_OBJECT_ID,
        initial_shared_version: SUI_SYSTEM_STATE_OBJECT_SHARED_VERSION,
        mutable: true,
    });
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let call_args = vec![
            system_object_arg.clone(),
            CallArg::Pure(bcs::to_bytes(&change_epoch.epoch).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.protocol_version.as_u64()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.storage_charge).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.computation_charge).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.storage_rebate).unwrap()),
            CallArg::Pure(bcs::to_bytes(&protocol_config.storage_fund_reinvest_rate()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&protocol_config.reward_slashing_rate()).unwrap()),
            CallArg::Pure(bcs::to_bytes(&change_epoch.epoch_start_timestamp_ms).unwrap()),
            CallArg::Pure(
                bcs::to_bytes(&get_sui_system_state_version(change_epoch.protocol_version))
                    .unwrap(),
            ),
        ];
        debug!(
            "Call arguments to advance_epoch transaction: {:?}",
            call_args
        );
        let res = builder.move_call(
            (*module_id.address()).into(),
            module_id.name().to_owned(),
            function,
            vec![],
            call_args,
        );
        assert_invariant!(res.is_ok(), "Unable to generate advance_epoch transaction!");
        builder.finish()
    };
    let result = programmable_transactions::execution::execute::<_, _, execution_mode::Normal>(
        protocol_config,
        move_vm,
        temporary_store,
        tx_ctx,
        gas_status,
        None,
        pt,
    );

    if result.is_err() {
        tracing::error!(
            "Failed to execute advance epoch transaction. Switching to safe mode. Error: {:?}. Input objects: {:?}. Tx data: {:?}",
            result.as_ref().err(),
            temporary_store.objects(),
            change_epoch,
        );
        temporary_store.drop_writes();
        let function = ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME.to_owned();
        let safe_mode_pt = {
            let mut builder = ProgrammableTransactionBuilder::new();
            let res = builder.move_call(
                (*module_id.address()).into(),
                module_id.name().to_owned(),
                function,
                vec![],
                vec![
                    system_object_arg,
                    CallArg::Pure(bcs::to_bytes(&change_epoch.epoch).unwrap()),
                    CallArg::Pure(bcs::to_bytes(&change_epoch.protocol_version).unwrap()),
                ],
            );
            assert_invariant!(
                res.is_ok(),
                "Unable to generate advance_epoch_safe_mode transaction!"
            );
            builder.finish()
        };
        programmable_transactions::execution::execute::<_, _, execution_mode::Normal>(
            protocol_config,
            move_vm,
            temporary_store,
            tx_ctx,
            gas_status,
            None,
            safe_mode_pt,
        )?;
    }

    for (version, modules) in change_epoch.system_packages.into_iter() {
        let modules = modules
            .into_iter()
            .map(|m| CompiledModule::deserialize(&m).unwrap())
            .collect();

        let mut new_package = Object::new_system_package(modules, version, tx_ctx.digest())?;

        info!(
            "upgraded system package {:?}",
            new_package.compute_object_reference()
        );

        // Decrement the version before writing the package so that the store can record the version
        // growing by one in the effects.
        new_package
            .data
            .try_as_package_mut()
            .unwrap()
            .decrement_version();
        temporary_store.write_object(new_package, WriteKind::Mutate);
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
) -> Result<(), ExecutionError> {
    let pt = {
        let mut builder = ProgrammableTransactionBuilder::new();
        let res = builder.move_call(
            SUI_FRAMEWORK_ADDRESS.into(),
            SUI_SYSTEM_MODULE_NAME.to_owned(),
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
    programmable_transactions::execution::execute::<_, _, execution_mode::Normal>(
        protocol_config,
        move_vm,
        temporary_store,
        tx_ctx,
        gas_status,
        None,
        pt,
    )
}
