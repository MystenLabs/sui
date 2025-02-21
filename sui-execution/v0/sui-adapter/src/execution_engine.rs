// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

pub use checked::*;

#[sui_macros::with_checked_arithmetic]
mod checked {
    use crate::execution_mode::{self, ExecutionMode};
    use crate::gas_charger::GasCharger;
    use crate::programmable_transactions;
    use crate::temporary_store::TemporaryStore;
    use crate::type_layout_resolver::TypeLayoutResolver;
    use move_binary_format::CompiledModule;
    use move_vm_runtime::move_vm::MoveVM;
    use std::{collections::HashSet, sync::Arc};
    use sui_protocol_config::{check_limit_by_meter, LimitThresholdCrossed, ProtocolConfig};
    use sui_types::balance::{
        BALANCE_CREATE_REWARDS_FUNCTION_NAME, BALANCE_DESTROY_REBATES_FUNCTION_NAME,
        BALANCE_MODULE_NAME,
    };
    use sui_types::clock::{CLOCK_MODULE_NAME, CONSENSUS_COMMIT_PROLOGUE_FUNCTION_NAME};
    use sui_types::committee::EpochId;
    use sui_types::effects::TransactionEffects;
    use sui_types::error::{ExecutionError, ExecutionErrorKind};
    use sui_types::execution::is_certificate_denied;
    use sui_types::execution_config_utils::to_binary_config;
    use sui_types::execution_status::ExecutionStatus;
    use sui_types::gas::GasCostSummary;
    use sui_types::gas::SuiGasStatus;
    use sui_types::gas_coin::GAS;
    use sui_types::inner_temporary_store::InnerTemporaryStore;
    use sui_types::messages_checkpoint::CheckpointTimestamp;
    use sui_types::metrics::LimitsMetrics;
    use sui_types::object::OBJECT_START_VERSION;
    use sui_types::programmable_transaction_builder::ProgrammableTransactionBuilder;
    use sui_types::storage::BackingStore;
    use sui_types::storage::WriteKind;
    #[cfg(msim)]
    use sui_types::sui_system_state::advance_epoch_result_injection::maybe_modify_result_legacy;
    use sui_types::sui_system_state::{AdvanceEpochParams, ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME};
    use sui_types::transaction::CheckedInputObjects;
    use sui_types::transaction::{
        Argument, CallArg, ChangeEpoch, Command, GenesisTransaction, ProgrammableTransaction,
        TransactionKind,
    };
    use sui_types::{
        base_types::{ObjectRef, SuiAddress, TransactionDigest, TxContext},
        object::{Object, ObjectInner},
        sui_system_state::{ADVANCE_EPOCH_FUNCTION_NAME, SUI_SYSTEM_MODULE_NAME},
        SUI_FRAMEWORK_ADDRESS,
    };
    use sui_types::{SUI_FRAMEWORK_PACKAGE_ID, SUI_SYSTEM_PACKAGE_ID};
    use tracing::{info, instrument, trace, warn};

    #[instrument(name = "tx_execute_to_effects", level = "debug", skip_all)]
    pub fn execute_transaction_to_effects<Mode: ExecutionMode>(
        store: &dyn BackingStore,
        input_objects: CheckedInputObjects,
        gas_coins: Vec<ObjectRef>,
        gas_status: SuiGasStatus,
        transaction_kind: TransactionKind,
        transaction_signer: SuiAddress,
        transaction_digest: TransactionDigest,
        move_vm: &Arc<MoveVM>,
        epoch_id: &EpochId,
        epoch_timestamp_ms: u64,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        certificate_deny_set: &HashSet<TransactionDigest>,
    ) -> (
        InnerTemporaryStore,
        SuiGasStatus,
        TransactionEffects,
        Result<Mode::ExecutionResults, ExecutionError>,
    ) {
        let input_objects = input_objects.into_inner();
        let shared_object_refs = input_objects.filter_shared_objects();
        let mut transaction_dependencies = input_objects.transaction_dependencies();
        let mut temporary_store =
            TemporaryStore::new(store, input_objects, transaction_digest, protocol_config);
        let mut gas_charger =
            GasCharger::new(transaction_digest, gas_coins, gas_status, protocol_config);

        let mut tx_ctx = TxContext::new_from_components(
            &transaction_signer,
            &transaction_digest,
            epoch_id,
            epoch_timestamp_ms,
        );

        let is_epoch_change = matches!(transaction_kind, TransactionKind::ChangeEpoch(_));

        let deny_cert = is_certificate_denied(&transaction_digest, certificate_deny_set);
        let (gas_cost_summary, execution_result) = execute_transaction::<Mode>(
            &mut temporary_store,
            transaction_kind,
            &mut gas_charger,
            &mut tx_ctx,
            move_vm,
            protocol_config,
            metrics,
            enable_expensive_checks,
            deny_cert,
            false,
        );

        let status = if let Err(error) = &execution_result {
            // Elaborate errors in logs if they are unexpected or their status is terse.
            use ExecutionErrorKind as K;
            match error.kind() {
                K::InvariantViolation | K::VMInvariantViolation => {
                    #[skip_checked_arithmetic]
                    tracing::error!(
                        kind = ?error.kind(),
                        tx_digest = ?transaction_digest,
                        "INVARIANT VIOLATION! Source: {:?}",
                        error.source(),
                    );
                }

                K::SuiMoveVerificationError | K::VMVerificationOrDeserializationError => {
                    #[skip_checked_arithmetic]
                    tracing::debug!(
                        kind = ?error.kind(),
                        tx_digest = ?transaction_digest,
                        "Verification Error. Source: {:?}",
                        error.source(),
                    );
                }

                K::PublishUpgradeMissingDependency | K::PublishUpgradeDependencyDowngrade => {
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
        transaction_dependencies.remove(&TransactionDigest::genesis_marker());

        if enable_expensive_checks && !Mode::allow_arbitrary_function_calls() {
            temporary_store
                .check_ownership_invariants(&transaction_signer, &mut gas_charger, is_epoch_change)
                .unwrap()
        } // else, in dev inspect mode and anything goes--don't check

        let (inner, effects) = temporary_store.to_effects(
            shared_object_refs,
            &transaction_digest,
            transaction_dependencies.into_iter().collect(),
            gas_cost_summary,
            status,
            &mut gas_charger,
            *epoch_id,
        );
        (
            inner,
            gas_charger.into_gas_status(),
            effects,
            execution_result,
        )
    }

    pub fn execute_genesis_state_update(
        store: &dyn BackingStore,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        move_vm: &Arc<MoveVM>,
        tx_context: &mut TxContext,
        input_objects: CheckedInputObjects,
        pt: ProgrammableTransaction,
    ) -> Result<InnerTemporaryStore, ExecutionError> {
        let input_objects = input_objects.into_inner();
        let mut temporary_store =
            TemporaryStore::new(store, input_objects, tx_context.digest(), protocol_config);
        let mut gas_charger = GasCharger::new_unmetered(tx_context.digest());
        programmable_transactions::execution::execute::<execution_mode::Genesis>(
            protocol_config,
            metrics,
            move_vm,
            &mut temporary_store,
            tx_context,
            &mut gas_charger,
            pt,
        )?;
        temporary_store.update_object_version_and_prev_tx();
        Ok(temporary_store.into_inner())
    }

    #[instrument(name = "tx_execute", level = "debug", skip_all)]
    fn execute_transaction<Mode: ExecutionMode>(
        temporary_store: &mut TemporaryStore<'_>,
        transaction_kind: TransactionKind,
        gas_charger: &mut GasCharger,
        tx_ctx: &mut TxContext,
        move_vm: &Arc<MoveVM>,
        protocol_config: &ProtocolConfig,
        metrics: Arc<LimitsMetrics>,
        enable_expensive_checks: bool,
        deny_cert: bool,
        contains_deleted_input: bool,
    ) -> (
        GasCostSummary,
        Result<Mode::ExecutionResults, ExecutionError>,
    ) {
        gas_charger.smash_gas(temporary_store);

        // At this point no charges have been applied yet
        debug_assert!(
            gas_charger.no_charges(),
            "No gas charges must be applied yet"
        );
        let is_genesis_tx = matches!(transaction_kind, TransactionKind::Genesis(_));
        let advance_epoch_gas_summary = transaction_kind.get_advance_epoch_tx_gas_summary();

        // We must charge object read here during transaction execution, because if this fails
        // we must still ensure an effect is committed and all objects versions incremented
        let result = gas_charger.charge_input_objects(temporary_store);
        let mut result = result.and_then(|()| {
            let mut execution_result = if deny_cert {
                Err(ExecutionError::new(
                    ExecutionErrorKind::CertificateDenied,
                    None,
                ))
            } else if contains_deleted_input {
                Err(ExecutionError::new(
                    ExecutionErrorKind::InputObjectDeleted,
                    None,
                ))
            } else {
                execution_loop::<Mode>(
                    temporary_store,
                    transaction_kind,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics.clone(),
                )
            };

            let effects_estimated_size = temporary_store.estimate_effects_size_upperbound();

            // Check if a limit threshold was crossed.
            // For metered transactions, there is not soft limit.
            // For system transactions, we allow a soft limit with alerting, and a hard limit where we terminate
            match check_limit_by_meter!(
                !gas_charger.is_unmetered(),
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
                if let (Some(normal_lim), Some(system_lim)) = (
                    protocol_config.max_size_written_objects_as_option(),
                    protocol_config.max_size_written_objects_system_tx_as_option(),
                ) {
                    let written_objects_size = temporary_store.written_objects_size();

                    match check_limit_by_meter!(
                        !gas_charger.is_unmetered(),
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

        let cost_summary = gas_charger.charge_gas(temporary_store, &mut result);
        // For advance epoch transaction, we need to provide epoch rewards and rebates as extra
        // information provided to check_sui_conserved, because we mint rewards, and burn
        // the rebates. We also need to pass in the unmetered_storage_rebate because storage
        // rebate is not reflected in the storage_rebate of gas summary. This is a bit confusing.
        // We could probably clean up the code a bit.
        // Put all the storage rebate accumulated in the system transaction
        // to the 0x5 object so that it's not lost.
        temporary_store.conserve_unmetered_storage_rebate(gas_charger.unmetered_storage_rebate());

        // === begin SUI conservation checks ===
        if !is_genesis_tx && !Mode::allow_arbitrary_values() {
            // ensure that this transaction did not create or destroy SUI, try to recover if the check fails
            let conservation_result = {
                let mut layout_resolver =
                    TypeLayoutResolver::new(move_vm, Box::new(&*temporary_store));
                temporary_store.check_sui_conserved(
                    &cost_summary,
                    advance_epoch_gas_summary,
                    &mut layout_resolver,
                    enable_expensive_checks,
                )
            };
            if let Err(conservation_err) = conservation_result {
                // conservation violated. try to avoid panic by dumping all writes, charging for gas, re-checking
                // conservation, and surfacing an aborted transaction with an invariant violation if all of that works
                result = Err(conservation_err);
                gas_charger.reset(temporary_store);
                gas_charger.charge_gas(temporary_store, &mut result);
                // check conservation once more more
                let mut layout_resolver =
                    TypeLayoutResolver::new(move_vm, Box::new(&*temporary_store));
                if let Err(recovery_err) = temporary_store.check_sui_conserved(
                    &cost_summary,
                    advance_epoch_gas_summary,
                    &mut layout_resolver,
                    enable_expensive_checks,
                ) {
                    // if we still fail, it's a problem with gas
                    // charging that happens even in the "aborted" case--no other option but panic.
                    // we will create or destroy SUI otherwise
                    panic!(
                        "SUI conservation fail in tx block {}: {}\nGas status is {}\nTx was ",
                        tx_ctx.digest(),
                        recovery_err,
                        gas_charger.summary()
                    )
                }
            }
        } // else, we're in the genesis transaction which mints the SUI supply, and hence does not satisfy SUI conservation, or
          // we're in the non-production dev inspect mode which allows us to violate conservation
          // === end SUI conservation checks ===
        (cost_summary, result)
    }

    fn execution_loop<Mode: ExecutionMode>(
        temporary_store: &mut TemporaryStore<'_>,
        transaction_kind: TransactionKind,
        tx_ctx: &mut TxContext,
        move_vm: &Arc<MoveVM>,
        gas_charger: &mut GasCharger,
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
                    gas_charger,
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
                        sui_types::transaction::GenesisObject::RawObject { data, owner } => {
                            let object = ObjectInner {
                                data,
                                owner,
                                previous_transaction: tx_ctx.digest(),
                                storage_rebate: 0,
                            };
                            temporary_store.write_object(object.into(), WriteKind::Create);
                        }
                    }
                }
                Ok(Mode::empty_results())
            }
            TransactionKind::ConsensusCommitPrologue(prologue) => {
                setup_consensus_commit(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                )
                .expect("ConsensusCommitPrologue cannot fail");
                Ok(Mode::empty_results())
            }
            TransactionKind::ConsensusCommitPrologueV2(prologue) => {
                setup_consensus_commit(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                )
                .expect("ConsensusCommitPrologue cannot fail");
                Ok(Mode::empty_results())
            }
            TransactionKind::ConsensusCommitPrologueV3(prologue) => {
                setup_consensus_commit(
                    prologue.commit_timestamp_ms,
                    temporary_store,
                    tx_ctx,
                    move_vm,
                    gas_charger,
                    protocol_config,
                    metrics,
                )
                .expect("ConsensusCommitPrologue cannot fail");
                Ok(Mode::empty_results())
            }
            TransactionKind::ProgrammableTransaction(pt) => {
                programmable_transactions::execution::execute::<Mode>(
                    protocol_config,
                    metrics,
                    move_vm,
                    temporary_store,
                    tx_ctx,
                    gas_charger,
                    pt,
                )
            }
            TransactionKind::AuthenticatorStateUpdate(_) => {
                panic!("AuthenticatorStateUpdate should not exist in execution layer v0");
            }
            TransactionKind::RandomnessStateUpdate(_) => {
                panic!("RandomnessStateUpdate should not exist in execution layer v0");
            }
            TransactionKind::EndOfEpochTransaction(_) => {
                panic!("EndOfEpochTransaction should not exist in execution layer v0");
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
            SUI_FRAMEWORK_PACKAGE_ID,
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
            SUI_FRAMEWORK_PACKAGE_ID,
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
        let call_arg_arguments = vec![
            CallArg::SUI_SYSTEM_MUT,
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

        info!("Call arguments to advance_epoch transaction: {:?}", params);

        let storage_rebates = builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ADVANCE_EPOCH_FUNCTION_NAME.to_owned(),
            vec![],
            arguments,
        );

        // Step 3: Destroy the storage rebates.
        builder.programmable_move_call(
            SUI_FRAMEWORK_PACKAGE_ID,
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

        let mut args = vec![
            CallArg::SUI_SYSTEM_MUT,
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

        info!("Call arguments to advance_epoch transaction: {:?}", params);

        builder.programmable_move_call(
            SUI_SYSTEM_PACKAGE_ID,
            SUI_SYSTEM_MODULE_NAME.to_owned(),
            ADVANCE_EPOCH_SAFE_MODE_FUNCTION_NAME.to_owned(),
            vec![],
            arguments,
        );

        Ok(builder.finish())
    }

    fn advance_epoch(
        change_epoch: ChangeEpoch,
        temporary_store: &mut TemporaryStore<'_>,
        tx_ctx: &mut TxContext,
        move_vm: &Arc<MoveVM>,
        gas_charger: &mut GasCharger,
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
        let result = programmable_transactions::execution::execute::<execution_mode::System>(
            protocol_config,
            metrics.clone(),
            move_vm,
            temporary_store,
            tx_ctx,
            gas_charger,
            advance_epoch_pt,
        );

        #[cfg(msim)]
        let result = maybe_modify_result_legacy(result, change_epoch.epoch);

        if result.is_err() {
            tracing::error!(
            "Failed to execute advance epoch transaction. Switching to safe mode. Error: {:?}. Input objects: {:?}. Tx data: {:?}",
            result.as_ref().err(),
            temporary_store.objects(),
            change_epoch,
        );
            temporary_store.drop_writes();
            // Must reset the storage rebate since we are re-executing.
            gas_charger.reset_storage_cost_and_rebate();

            if protocol_config.get_advance_epoch_start_time_in_safe_mode() {
                temporary_store.advance_epoch_safe_mode(&params, protocol_config);
            } else {
                let advance_epoch_safe_mode_pt =
                    construct_advance_epoch_safe_mode_pt(&params, protocol_config)?;
                programmable_transactions::execution::execute::<execution_mode::System>(
                    protocol_config,
                    metrics.clone(),
                    move_vm,
                    temporary_store,
                    tx_ctx,
                    gas_charger,
                    advance_epoch_safe_mode_pt,
                )
                .expect("Advance epoch with safe mode must succeed");
            }
        }

        let binary_config = to_binary_config(protocol_config);
        for (version, modules, dependencies) in change_epoch.system_packages.into_iter() {
            let deserialized_modules: Vec<_> = modules
                .iter()
                .map(|m| CompiledModule::deserialize_with_config(m, &binary_config).unwrap())
                .collect();

            if version == OBJECT_START_VERSION {
                let package_id = deserialized_modules.first().unwrap().address();
                info!("adding new system package {package_id}");

                let publish_pt = {
                    let mut b = ProgrammableTransactionBuilder::new();
                    b.command(Command::Publish(modules, dependencies));
                    b.finish()
                };

                programmable_transactions::execution::execute::<execution_mode::System>(
                    protocol_config,
                    metrics.clone(),
                    move_vm,
                    temporary_store,
                    tx_ctx,
                    gas_charger,
                    publish_pt,
                )
                .expect("System Package Publish must succeed");
            } else {
                let mut new_package = Object::new_system_package(
                    &deserialized_modules,
                    version,
                    dependencies,
                    tx_ctx.digest(),
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
    fn setup_consensus_commit(
        consensus_commit_timestamp_ms: CheckpointTimestamp,
        temporary_store: &mut TemporaryStore<'_>,
        tx_ctx: &mut TxContext,
        move_vm: &Arc<MoveVM>,
        gas_charger: &mut GasCharger,
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
                    CallArg::CLOCK_MUT,
                    CallArg::Pure(bcs::to_bytes(&consensus_commit_timestamp_ms).unwrap()),
                ],
            );
            assert_invariant!(
                res.is_ok(),
                "Unable to generate consensus_commit_prologue transaction!"
            );
            builder.finish()
        };
        programmable_transactions::execution::execute::<execution_mode::System>(
            protocol_config,
            metrics,
            move_vm,
            temporary_store,
            tx_ctx,
            gas_charger,
            pt,
        )
    }
}
