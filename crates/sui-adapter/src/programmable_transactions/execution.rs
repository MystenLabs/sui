// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use move_binary_format::{
    access::ModuleAccess,
    errors::{Location, PartialVMResult, VMResult},
    file_format::{AbilitySet, CodeOffset, FunctionDefinitionIndex, LocalIndex, Visibility},
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::IdentStr,
    language_storage::{ModuleId, StructTag, TypeTag},
    u256::U256,
};
use move_vm_runtime::{
    move_vm::MoveVM,
    session::{LoadedFunctionInstantiation, SerializedReturnValues},
};
use move_vm_types::loaded_data::runtime_types::{StructType, Type};
use serde::{de::DeserializeSeed, Deserialize};
use sui_move_natives::object_runtime::ObjectRuntime;
use sui_protocol_config::ProtocolConfig;
use sui_types::execution_status::{CommandArgumentError, PackageUpgradeError};
use sui_types::{
    base_types::{
        MoveObjectType, ObjectID, SuiAddress, TxContext, TxContextKind, RESOLVED_ASCII_STR,
        RESOLVED_STD_OPTION, RESOLVED_UTF8_STR, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME,
    },
    coin::Coin,
    error::{ExecutionError, ExecutionErrorKind},
    event::Event,
    gas::{SuiGasStatus, SuiGasStatusAPI},
    id::{RESOLVED_SUI_ID, UID},
    messages::{Argument, Command, ProgrammableMoveCall, ProgrammableTransaction},
    metrics::LimitsMetrics,
    move_package::{
        normalize_deserialized_modules, MovePackage, TypeOrigin, UpgradeCap, UpgradePolicy,
        UpgradeReceipt, UpgradeTicket,
    },
    Identifier, SUI_FRAMEWORK_ADDRESS,
};
use sui_verifier::{
    private_generics::{EVENT_MODULE, PRIVATE_TRANSFER_FUNCTIONS, TRANSFER_MODULE},
    INIT_FN_NAME,
};

use crate::{adapter::substitute_package_id, execution_mode::ExecutionMode};

use super::{context::*, types::*};

sui_macros::checked_arithmetic! {

pub fn execute<S: StorageView, Mode: ExecutionMode>(
    protocol_config: &ProtocolConfig,
    metrics: Arc<LimitsMetrics>,
    vm: &MoveVM,
    state_view: &mut S,
    tx_context: &mut TxContext,
    gas_status: &mut SuiGasStatus,
    gas_coin: Option<ObjectID>,
    pt: ProgrammableTransaction,
) -> Result<Mode::ExecutionResults, ExecutionError> {
    let ProgrammableTransaction { inputs, commands } = pt;
    let mut context = ExecutionContext::new(
        protocol_config,
        metrics,
        vm,
        state_view,
        tx_context,
        gas_status,
        gas_coin,
        inputs,
    )?;
    // execute commands
    let mut mode_results = Mode::empty_results();
    for (idx, command) in commands.into_iter().enumerate() {
        if let Err(err) = execute_command::<_, Mode>(&mut context, &mut mode_results, command) {
                let object_runtime: &ObjectRuntime = context.session.get_native_extensions().get();
                // We still need to record the loaded child objects for replay
                let loaded_child_objects = object_runtime.loaded_child_objects();
                drop(context);
                state_view.save_loaded_child_objects(loaded_child_objects);
                return Err(err.with_command_index(idx));
        };
    }

    // Save loaded objects table in case we fail in post execution
    let object_runtime: &ObjectRuntime = context.session.get_native_extensions().get();
    // We still need to record the loaded child objects for replay
    let loaded_child_objects = object_runtime.loaded_child_objects();

    // apply changes
    let finished  = context.finish::<Mode>();
    // Save loaded objects for debug. We dont want to lose the info
    state_view.save_loaded_child_objects(loaded_child_objects);

    let ExecutionResults {
        object_changes,
        user_events,
    } =  finished?;
    state_view.apply_object_changes(object_changes);
    for (module_id, tag, contents) in user_events {
        state_view.log_event(Event::new(
            module_id.address(),
            module_id.name(),
            tx_context.sender(),
            tag,
            contents,
        ))
    }
    Ok(mode_results)
}

/// Execute a single command
fn execute_command<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    mode_results: &mut Mode::ExecutionResults,
    command: Command,
) -> Result<(), ExecutionError> {
    let mut argument_updates = Mode::empty_arguments();
    let results = match command {
        Command::MakeMoveVec(tag_opt, args) if args.is_empty() => {
            let Some(tag) = tag_opt else {
                invariant_violation!(
                    "input checker ensures if args are empty, there is a type specified"
                );
            };
            let elem_ty = context
                .load_type(&tag)
                .map_err(|e| context.convert_vm_error(e))?;
            let ty = Type::Vector(Box::new(elem_ty));
            let abilities = context
                .session
                .get_type_abilities(&ty)
                .map_err(|e| context.convert_vm_error(e))?;
            // BCS layout for any empty vector should be the same
            let bytes = bcs::to_bytes::<Vec<u8>>(&vec![]).unwrap();
            vec![Value::Raw(
                RawValueType::Loaded {
                    ty,
                    abilities,
                    used_in_non_entry_move_call: false,
                },
                bytes,
            )]
        }
        Command::MakeMoveVec(tag_opt, args) => {
            let mut res = vec![];
            leb128::write::unsigned(&mut res, args.len() as u64).unwrap();
            let mut arg_iter = args.into_iter().enumerate();
            let (mut used_in_non_entry_move_call, elem_ty) = match tag_opt {
                Some(tag) => {
                    let elem_ty = context
                        .load_type(&tag)
                        .map_err(|e| context.convert_vm_error(e))?;
                    (false, elem_ty)
                }
                // If no tag specified, it _must_ be an object
                None => {
                    // empty args covered above
                    let (idx, arg) = arg_iter.next().unwrap();
                    let obj: ObjectValue =
                        context.by_value_arg(CommandKind::MakeMoveVec, idx, arg)?;
                    obj.write_bcs_bytes(&mut res);
                    (obj.used_in_non_entry_move_call, obj.type_)
                }
            };
            for (idx, arg) in arg_iter {
                let value: Value = context.by_value_arg(CommandKind::MakeMoveVec, idx, arg)?;
                check_param_type::<_, Mode>(context, idx, &value, &elem_ty)?;
                used_in_non_entry_move_call =
                    used_in_non_entry_move_call || value.was_used_in_non_entry_move_call();
                value.write_bcs_bytes(&mut res);
            }
            let ty = Type::Vector(Box::new(elem_ty));
            let abilities = context
                .session
                .get_type_abilities(&ty)
                .map_err(|e| context.convert_vm_error(e))?;
            vec![Value::Raw(
                RawValueType::Loaded {
                    ty,
                    abilities,
                    used_in_non_entry_move_call,
                },
                res,
            )]
        }
        Command::TransferObjects(objs, addr_arg) => {
            let objs: Vec<ObjectValue> = objs
                .into_iter()
                .enumerate()
                .map(|(idx, arg)| context.by_value_arg(CommandKind::TransferObjects, idx, arg))
                .collect::<Result<_, _>>()?;
            let addr: SuiAddress =
                context.by_value_arg(CommandKind::TransferObjects, objs.len(), addr_arg)?;
            for obj in objs {
                obj.ensure_public_transfer_eligible()?;
                context.transfer_object(obj, addr)?;
            }
            vec![]
        }
        Command::SplitCoins(coin_arg, amount_args) => {
            let mut obj: ObjectValue = context.borrow_arg_mut(0, coin_arg)?;
            let ObjectContents::Coin(coin) = &mut obj.contents else {
                let e = ExecutionErrorKind::command_argument_error(
                    CommandArgumentError::TypeMismatch,
                    0,
                );
                let msg = "Expected a coin but got an non coin object".to_owned();
                return Err(ExecutionError::new_with_source(e, msg));
            };
            let split_coins = amount_args
                .into_iter()
                .map(|amount_arg| {
                    let amount: u64 =
                        context.by_value_arg(CommandKind::SplitCoins, 1, amount_arg)?;
                    let new_coin_id = context.fresh_id()?;
                    let new_coin = coin.split(amount, UID::new(new_coin_id))?;
                    let coin_type = obj.type_.clone();
                    // safe because we are propagating the coin type, and relying on the internal
                    // invariant that coin values have a coin type
                    let new_coin = unsafe { ObjectValue::coin(coin_type, new_coin) };
                    Ok(Value::Object(new_coin))
                })
                .collect::<Result<_, ExecutionError>>()?;
            context.restore_arg::<Mode>(&mut argument_updates, coin_arg, Value::Object(obj))?;
            split_coins
        }
        Command::MergeCoins(target_arg, coin_args) => {
            let mut target: ObjectValue = context.borrow_arg_mut(0, target_arg)?;
            let ObjectContents::Coin(target_coin) = &mut target.contents else {
                let e = ExecutionErrorKind::command_argument_error(
                    CommandArgumentError::TypeMismatch,
                    0,
                );
                let msg = "Expected a coin but got an non coin object".to_owned();
                return Err(ExecutionError::new_with_source(e, msg));
            };
            let coins: Vec<ObjectValue> = coin_args
                .into_iter()
                .enumerate()
                .map(|(idx, arg)| context.by_value_arg(CommandKind::MergeCoins, idx + 1, arg))
                .collect::<Result<_, _>>()?;
            for (idx, coin) in coins.into_iter().enumerate() {
                if target.type_ != coin.type_ {
                    let e = ExecutionErrorKind::command_argument_error(
                        CommandArgumentError::TypeMismatch,
                        (idx + 1) as u16,
                    );
                    let msg = "Coins do not have the same type".to_owned();
                    return Err(ExecutionError::new_with_source(e, msg));
                }
                let ObjectContents::Coin(Coin { id, balance }) = coin.contents else {
                    invariant_violation!(
                        "Target coin was a coin, and we already checked for the same type. \
                        This should be a coin"
                    );
                };
                context.delete_id(*id.object_id())?;
                target_coin.add(balance)?;
            }
            context.restore_arg::<Mode>(
                &mut argument_updates,
                target_arg,
                Value::Object(target),
            )?;
            vec![]
        }
        Command::MoveCall(move_call) => {
            let ProgrammableMoveCall {
                package,
                module,
                function,
                type_arguments,
                arguments,
            } = *move_call;

            // Convert type arguments to `Type`s
            let mut loaded_type_arguments = Vec::with_capacity(type_arguments.len());
            for (ix, type_arg) in type_arguments.into_iter().enumerate() {
                let ty = context
                    .load_type(&type_arg)
                    .map_err(|e| context.convert_type_argument_error(ix, e))?;
                loaded_type_arguments.push(ty);
            }

            let original_address = context.set_link_context(package)?;
            let runtime_id = ModuleId::new(original_address, module);
            let return_values = execute_move_call::<_, Mode>(
                context,
                &mut argument_updates,
                &runtime_id,
                &function,
                loaded_type_arguments,
                arguments,
                /* is_init */ false,
            );

            context.reset_linkage();
            return_values?
        }
        Command::Publish(modules, dep_ids) => {
            execute_move_publish::<_, Mode>(context, &mut argument_updates, modules, dep_ids)?
        }
        Command::Upgrade(modules, dep_ids, current_package_id, upgrade_ticket) => {
            execute_move_upgrade::<_, Mode>(
                context,
                modules,
                dep_ids,
                current_package_id,
                upgrade_ticket,
            )?
        }
    };

    Mode::finish_command(context, mode_results, argument_updates, &results)?;
    context.push_command_results(results)?;
    Ok(())
}

/// Execute a single Move call
fn execute_move_call<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    argument_updates: &mut Mode::ArgumentUpdates,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: Vec<Type>,
    arguments: Vec<Argument>,
    is_init: bool,
) -> Result<Vec<Value>, ExecutionError> {
    // check that the function is either an entry function or a valid public function
    let LoadedFunctionInfo {
        kind,
        signature,
        return_value_kinds,
        index,
        last_instr,
    } = check_visibility_and_signature::<_, Mode>(
        context,
        module_id,
        function,
        &type_arguments,
        is_init,
    )?;
    // build the arguments, storing meta data about by-mut-ref args
    let (tx_context_kind, by_mut_ref, serialized_arguments) =
        build_move_args::<_, Mode>(context, module_id, function, kind, &signature, &arguments)?;
    // invoke the VM
    let SerializedReturnValues {
        mutable_reference_outputs,
        return_values,
    } = vm_move_call(
        context,
        module_id,
        function,
        type_arguments,
        tx_context_kind,
        serialized_arguments,
    )?;
    assert_invariant!(
        by_mut_ref.len() == mutable_reference_outputs.len(),
        "lost mutable input"
    );

    context.take_user_events(module_id, index, last_instr)?;

    // save the link context because calls to `make_value` below can set new ones, and we don't want
    // it to be clobbered.
    let saved_linkage = context.steal_linkage();
    // write back mutable inputs. We also update if they were used in non entry Move calls
    // though we do not care for immutable usages of objects or other values
    let used_in_non_entry_move_call = kind == FunctionKind::NonEntry;
    let res = write_back_results::<_, Mode>(
        context,
        argument_updates,
        &arguments,
        used_in_non_entry_move_call,
        mutable_reference_outputs
            .into_iter()
            .map(|(i, bytes, _layout)| (i, bytes)),
        by_mut_ref,
        return_values.into_iter().map(|(bytes, _layout)| bytes),
        return_value_kinds,
    );

    context.restore_linkage(saved_linkage)?;
    res
}

fn write_back_results<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    argument_updates: &mut Mode::ArgumentUpdates,
    arguments: &[Argument],
    non_entry_move_call: bool,
    mut_ref_values: impl IntoIterator<Item = (u8, Vec<u8>)>,
    mut_ref_kinds: impl IntoIterator<Item = (u8, ValueKind)>,
    return_values: impl IntoIterator<Item = Vec<u8>>,
    return_value_kinds: impl IntoIterator<Item = ValueKind>,
) -> Result<Vec<Value>, ExecutionError> {
    for ((i, bytes), (j, kind)) in mut_ref_values.into_iter().zip(mut_ref_kinds) {
        assert_invariant!(i == j, "lost mutable input");
        let arg_idx = i as usize;
        let value = make_value(context, kind, bytes, non_entry_move_call)?;
        context.restore_arg::<Mode>(argument_updates, arguments[arg_idx], value)?;
    }

    return_values
        .into_iter()
        .zip(return_value_kinds)
        .map(|(bytes, kind)| {
            // only non entry functions have return values
            make_value(
                context, kind, bytes, /* used_in_non_entry_move_call */ true,
            )
        })
        .collect()
}

fn make_value<S: StorageView>(
    context: &mut ExecutionContext<S>,
    value_info: ValueKind,
    bytes: Vec<u8>,
    used_in_non_entry_move_call: bool,
) -> Result<Value, ExecutionError> {
    Ok(match value_info {
        ValueKind::Object {
            type_,
            has_public_transfer,
        } => Value::Object(ObjectValue::new(
            context.vm,
            &mut context.session,
            type_,
            has_public_transfer,
            used_in_non_entry_move_call,
            &bytes,
        )?),
        ValueKind::Raw(ty, abilities) => Value::Raw(
            RawValueType::Loaded {
                ty,
                abilities,
                used_in_non_entry_move_call,
            },
            bytes,
        ),
    })
}

/// Publish Move modules and call the init functions.  Returns an `UpgradeCap` for the newly
/// published package on success.
fn execute_move_publish<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    argument_updates: &mut Mode::ArgumentUpdates,
    module_bytes: Vec<Vec<u8>>,
    dep_ids: Vec<ObjectID>,
) -> Result<Vec<Value>, ExecutionError> {
    assert_invariant!(
        !module_bytes.is_empty(),
        "empty package is checked in transaction input checker"
    );
    context
        .gas_status
        .charge_publish_package(module_bytes.iter().map(|v| v.len()).sum())?;

    let mut modules = deserialize_modules::<_, Mode>(context, &module_bytes)?;

    // It should be fine that this does not go through ExecutionContext::fresh_id since the Move
    // runtime does not to know about new packages created, since Move objects and Move packages
    // cannot interact
    let runtime_id = if Mode::packages_are_predefined() {
        // do not calculate or substitute id for predefined packages
        (*modules[0].self_id().address()).into()
    } else {
        let id = context.tx_context.fresh_id();
        substitute_package_id(&mut modules, id)?;
        id
    };

    // For newly published packages, runtime ID matches storage ID.
    let storage_id = runtime_id;

    // Preserve the old order of operations when package upgrades are not supported, because it
    // affects the order in which error cases are checked.
    let package_obj = if context.protocol_config.package_upgrades_supported() {
        let dependencies = fetch_packages(context, &dep_ids)?;
        let package_obj = context.new_package(&modules, &dependencies)?;

        let Some(package) = package_obj.data.try_as_package() else {
            invariant_violation!("Newly created package object is not a package");
        };

        context.set_linkage(package)?;
        let res = publish_and_verify_modules(context, runtime_id, &modules)
            .and_then(|_| init_modules::<_, Mode>(context, argument_updates, &modules));
        context.reset_linkage();
        res?;

        package_obj
    } else {
        // FOR THE LOVE OF ALL THAT IS GOOD DO NOT RE-ORDER THIS.  It looks redundant, but is
        // required to maintain backwards compatibility.
        publish_and_verify_modules(context, runtime_id, &modules)?;
        let dependencies = fetch_packages(context, &dep_ids)?;
        let package = context.new_package(&modules, &dependencies)?;
        init_modules::<_, Mode>(context, argument_updates, &modules)?;
        package
    };

    context.write_package(package_obj)?;
    let values = if Mode::packages_are_predefined() {
        // no upgrade cap for genesis modules
        vec![]
    } else {
        let cap = &UpgradeCap::new(context.fresh_id()?, storage_id);
        vec![Value::Object(ObjectValue::new(
            context.vm,
            &mut context.session,
            UpgradeCap::type_().into(),
            /* has_public_transfer */ true,
            /* used_in_non_entry_move_call */ false,
            &bcs::to_bytes(cap).unwrap(),
        )?)]
    };
    Ok(values)
}

/// Upgrade a Move package.  Returns an `UpgradeReceipt` for the upgraded package on success.
fn execute_move_upgrade<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    module_bytes: Vec<Vec<u8>>,
    dep_ids: Vec<ObjectID>,
    current_package_id: ObjectID,
    upgrade_ticket_arg: Argument,
) -> Result<Vec<Value>, ExecutionError> {
    // Check that package upgrades are supported.
    context
        .protocol_config
        .check_package_upgrades_supported()
        .map_err(|_| ExecutionError::from_kind(ExecutionErrorKind::FeatureNotYetSupported))?;

    assert_invariant!(
        !module_bytes.is_empty(),
        "empty package is checked in transaction input checker"
    );

    let upgrade_ticket_type = context
        .load_type(&TypeTag::Struct(Box::new(UpgradeTicket::type_())))
        .map_err(|e| context.convert_vm_error(e))?;
    let upgrade_receipt_type = context
        .load_type(&TypeTag::Struct(Box::new(UpgradeReceipt::type_())))
        .map_err(|e| context.convert_vm_error(e))?;

    let upgrade_ticket: UpgradeTicket = {
        let mut ticket_bytes = Vec::new();
        let ticket_val: Value =
            context.by_value_arg(CommandKind::Upgrade, 0, upgrade_ticket_arg)?;
        check_param_type::<_, Mode>(context, 0, &ticket_val, &upgrade_ticket_type)?;
        ticket_val.write_bcs_bytes(&mut ticket_bytes);
        bcs::from_bytes(&ticket_bytes).map_err(|_| {
            ExecutionError::from_kind(ExecutionErrorKind::CommandArgumentError {
                arg_idx: 0,
                kind: CommandArgumentError::InvalidBCSBytes,
            })
        })?
    };

    // Make sure the passed-in package ID matches the package ID in the `upgrade_ticket`.
    if current_package_id != upgrade_ticket.package.bytes {
        return Err(ExecutionError::from_kind(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::PackageIDDoesNotMatch {
                    package_id: current_package_id,
                    ticket_id: upgrade_ticket.package.bytes,
                },
            },
        ));
    }

    // Check digest.
    let computed_digest =
        MovePackage::compute_digest_for_modules_and_deps(
            &module_bytes,
            &dep_ids,
            context.protocol_config.package_digest_hash_module(),
        ).to_vec();
    if computed_digest != upgrade_ticket.digest {
        return Err(ExecutionError::from_kind(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::DigestDoesNotMatch {
                    digest: computed_digest,
                },
            },
        ));
    }

    // Check that this package ID points to a package and get the package we're upgrading.
    let current_package = fetch_package(context, &upgrade_ticket.package.bytes)?;

    let mut modules = deserialize_modules::<_, Mode>(context, &module_bytes)?;
    let runtime_id = current_package.original_package_id();
    substitute_package_id(&mut modules, runtime_id)?;

    // Upgraded packages share their predecessor's runtime ID but get a new storage ID.
    let storage_id = context.tx_context.fresh_id();

    let dependencies = fetch_packages(context, &dep_ids)?;
    let package_obj =
        context.upgrade_package(storage_id, &current_package, &modules, &dependencies)?;

    let Some(package) = package_obj.data.try_as_package() else {
        invariant_violation!("Newly created package object is not a package");
    };

    // Populate loader with all previous types.
    if !context.protocol_config.disallow_adding_abilities_on_upgrade() {
        for TypeOrigin { module_name, struct_name, package: origin } in package.type_origin_table() {
            if package.id() == *origin {
                continue;
            }

            let Ok(module) = Identifier::new(module_name.as_str()) else {
                continue;
            };

            let Ok(name) = Identifier::new(struct_name.as_str()) else {
                continue;
            };

            let _ = context.load_type(&TypeTag::Struct(Box::new(StructTag {
                address: (*origin).into(),
                module,
                name,
                type_params: vec![],
            })));
        }
    }

    context.set_linkage(package)?;
    let res = publish_and_verify_modules(context, runtime_id, &modules);
    context.reset_linkage();
    res?;

    check_compatibility(context, &current_package, &modules, upgrade_ticket.policy)?;

    context.write_package(package_obj)?;
    Ok(vec![Value::Raw(
        RawValueType::Loaded {
            ty: upgrade_receipt_type,
            abilities: AbilitySet::EMPTY,
            used_in_non_entry_move_call: false,
        },
        bcs::to_bytes(&UpgradeReceipt::new(upgrade_ticket, storage_id)).unwrap(),
    )])
}

fn check_compatibility<'a, S: StorageView>(
    context: &ExecutionContext<S>,
    existing_package: &MovePackage,
    upgrading_modules: impl IntoIterator<Item = &'a CompiledModule>,
    policy: u8,
) -> Result<(), ExecutionError> {
    // Make sure this is a known upgrade policy.
    let Ok(policy) = UpgradePolicy::try_from(policy) else {
        return Err(ExecutionError::from_kind(
            ExecutionErrorKind::PackageUpgradeError {
                upgrade_error: PackageUpgradeError::UnknownUpgradePolicy { policy },
            },
        ));
    };

    let Ok(current_normalized) = existing_package.normalize(context.protocol_config.move_binary_format_version(), context.protocol_config.no_extraneous_module_bytes()) else {
        invariant_violation!("Tried to normalize modules in existing package but failed")
    };

    let mut new_normalized = normalize_deserialized_modules(upgrading_modules.into_iter());
    for (name, cur_module) in current_normalized {
        let msg = format!("Existing module {name} not found in next version of package");
        let Some(new_module) = new_normalized.remove(&name) else {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PackageUpgradeError {
                    upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
                },
                msg,
            ));
        };

        if let Err(e) = policy.check_compatibility(&cur_module, &new_module, context.protocol_config) {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PackageUpgradeError {
                    upgrade_error: PackageUpgradeError::IncompatibleUpgrade,
                },
                e,
            ));
        }
    }

    Ok(())
}

fn fetch_package<S: StorageView>(
    context: &ExecutionContext<S>,
    package_id: &ObjectID,
) -> Result<MovePackage, ExecutionError> {
    let mut fetched_packages = fetch_packages(context, vec![package_id])?;
    assert_invariant!(
        fetched_packages.len() == 1,
        "Number of fetched packages must match the number of package object IDs if successful."
    );
    match fetched_packages.pop() {
        Some(pkg) => Ok(pkg),
        None => invariant_violation!(
            "We should always fetch a package for each object or return a dependency error."
        ),
    }
}

fn fetch_packages<'a, S: StorageView>(
    context: &'a ExecutionContext<S>,
    package_ids: impl IntoIterator<Item = &'a ObjectID>,
) -> Result<Vec<MovePackage>, ExecutionError> {
    let package_ids: BTreeSet<_> = package_ids.into_iter().collect();
    match context.state_view.get_packages(package_ids) {
        Err(e) => Err(ExecutionError::new_with_source(
            ExecutionErrorKind::PublishUpgradeMissingDependency,
            e,
        )),
        Ok(Err(missing_deps)) => {
            let msg = format!(
                "Missing dependencies: {}",
                missing_deps
                    .into_iter()
                    .map(|dep| format!("{}", dep))
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishUpgradeMissingDependency,
                msg,
            ))
        }
        Ok(Ok(pkgs)) => Ok(pkgs),
    }
}

/***************************************************************************************************
 * Move execution
 **************************************************************************************************/

fn vm_move_call<S: StorageView>(
    context: &mut ExecutionContext<S>,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: Vec<Type>,
    tx_context_kind: TxContextKind,
    mut serialized_arguments: Vec<Vec<u8>>,
) -> Result<SerializedReturnValues, ExecutionError> {
    match tx_context_kind {
        TxContextKind::None => (),
        TxContextKind::Mutable | TxContextKind::Immutable => {
            serialized_arguments.push(context.tx_context.to_vec());
        }
    }
    // script visibility checked manually for entry points
    let mut result = context
        .session
        .execute_function_bypass_visibility(
            module_id,
            function,
            type_arguments,
            serialized_arguments,
            context.gas_status.move_gas_status(),
        )
        .map_err(|e| context.convert_vm_error(e))?;

    // When this function is used during publishing, it
    // may be executed several times, with objects being
    // created in the Move VM in each Move call. In such
    // case, we need to update TxContext value so that it
    // reflects what happened each time we call into the
    // Move VM (e.g. to account for the number of created
    // objects).
    if tx_context_kind == TxContextKind::Mutable {
        let Some((_, ctx_bytes, _)) = result.mutable_reference_outputs.pop() else {
            invariant_violation!("Missing TxContext in reference outputs");
        };
        let updated_ctx: TxContext = bcs::from_bytes(&ctx_bytes).map_err(|e| {
            ExecutionError::invariant_violation(format!(
                "Unable to deserialize TxContext bytes. {e}"
            ))
        })?;
        context.tx_context.update_state(updated_ctx)?;
    }
    Ok(result)
}

fn deserialize_modules<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    module_bytes: &[Vec<u8>],
) -> Result<Vec<CompiledModule>, ExecutionError> {
    let modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize_with_config(
                b,
                context.protocol_config.move_binary_format_version(),
                context.protocol_config.no_extraneous_module_bytes(),
            )
            .map_err(|e| e.finish(Location::Undefined))
        })
        .collect::<VMResult<Vec<CompiledModule>>>()
        .map_err(|e| context.convert_vm_error(e))?;

    assert_invariant!(
        !modules.is_empty(),
        "input checker ensures package is not empty"
    );

    Ok(modules)
}

fn publish_and_verify_modules<S: StorageView>(
    context: &mut ExecutionContext<S>,
    package_id: ObjectID,
    modules: &[CompiledModule],
) -> Result<(), ExecutionError> {
    // TODO(https://github.com/MystenLabs/sui/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
    let new_module_bytes: Vec<_> = modules
        .iter()
        .map(|m| {
            let mut bytes = Vec::new();
            m.serialize(&mut bytes).unwrap();
            bytes
        })
        .collect();
    context
        .session
        .publish_module_bundle(
            new_module_bytes,
            AccountAddress::from(package_id),
            // TODO: publish_module_bundle() currently doesn't charge gas.
            // Do we want to charge there?
            context.gas_status.move_gas_status(),
        )
        .map_err(|e| context.convert_vm_error(e))?;

    // run the Sui verifier
    for module in modules {
        // Run Sui bytecode verifier, which runs some additional checks that assume the Move
        // bytecode verifier has passed.
        sui_verifier::verifier::sui_verify_module_unmetered(context.protocol_config, module, &BTreeMap::new())?;
    }

    Ok(())
}

fn init_modules<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    argument_updates: &mut Mode::ArgumentUpdates,
    modules: &[CompiledModule],
) -> Result<(), ExecutionError> {
    let modules_to_init = modules.iter().filter_map(|module| {
        for fdef in &module.function_defs {
            let fhandle = module.function_handle_at(fdef.function);
            let fname = module.identifier_at(fhandle.name);
            if fname == INIT_FN_NAME {
                return Some(module.self_id());
            }
        }
        None
    });

    for module_id in modules_to_init {
        let return_values = execute_move_call::<_, Mode>(
            context,
            argument_updates,
            &module_id,
            INIT_FN_NAME,
            vec![],
            vec![],
            /* is_init */ true,
        )?;

        assert_invariant!(
            return_values.is_empty(),
            "init should not have return values"
        )
    }

    Ok(())
}

/***************************************************************************************************
 * Move signatures
 **************************************************************************************************/

/// Helper marking what function we are invoking
#[derive(PartialEq, Eq, Clone, Copy)]
enum FunctionKind {
    PrivateEntry,
    PublicEntry,
    NonEntry,
    Init,
}

/// Used to remember type information about a type when resolving the signature
enum ValueKind {
    Object {
        type_: MoveObjectType,
        has_public_transfer: bool,
    },
    Raw(Type, AbilitySet),
}

struct LoadedFunctionInfo {
    /// The kind of the function, e.g. public or private or init
    kind: FunctionKind,
    /// The signature information of the function
    signature: LoadedFunctionInstantiation,
    /// Object or type information for the return values
    return_value_kinds: Vec<ValueKind>,
    /// Definition index of the function
    index: FunctionDefinitionIndex,
    /// The length of the function used for setting error information, or 0 if native
    last_instr: CodeOffset,
}

/// Checks that the function to be called is either
/// - an entry function
/// - a public function that does not return references
/// - module init (only internal usage)
fn check_visibility_and_signature<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    module_id: &ModuleId,
    function: &IdentStr,
    type_arguments: &[Type],
    from_init: bool,
) -> Result<LoadedFunctionInfo, ExecutionError> {
    if from_init {
        // the session is weird and does not load the module on publishing. This is a temporary
        // work around, since loading the function through the session will cause the module
        // to be loaded through the sessions data store.
        let result = context
            .session
            .load_function(module_id, function, type_arguments);
        assert_invariant!(
            result.is_ok(),
            "The modules init should be able to be loaded"
        );
    }
    let module = context
        .vm
        .load_module(module_id, context.session.get_resolver())
        .map_err(|e| context.convert_vm_error(e))?;
    let Some((index, fdef)) = module.function_defs.iter().enumerate().find(|(_index, fdef)| {
        module.identifier_at(module.function_handle_at(fdef.function).name) == function
    }) else {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::FunctionNotFound,
            format!(
                "Could not resolve function '{}' in module {}",
                function, &module_id,
            ),
        ));
    };

    // entry on init is now banned, so ban invoking it
    if !from_init && function == INIT_FN_NAME && context.protocol_config.ban_entry_init() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            "Cannot call 'init'",
        ));
    }

    let last_instr: CodeOffset = fdef
        .code
        .as_ref()
        .map(|code| code.code.len() - 1)
        .unwrap_or(0) as CodeOffset;
    let function_kind = match (fdef.visibility, fdef.is_entry) {
        (Visibility::Private | Visibility::Friend, true) => FunctionKind::PrivateEntry,
        (Visibility::Public, true) => FunctionKind::PublicEntry,
        (Visibility::Public, false) => FunctionKind::NonEntry,
        (Visibility::Private, false) if from_init => {
            assert_invariant!(
                function == INIT_FN_NAME,
                "module init specified non-init function"
            );
            FunctionKind::Init
        }
        (Visibility::Private | Visibility::Friend, false)
            if Mode::allow_arbitrary_function_calls() =>
        {
            FunctionKind::NonEntry
        }
        (Visibility::Private | Visibility::Friend, false) => {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::NonEntryFunctionInvoked,
                "Can only call `entry` or `public` functions",
            ));
        }
    };
    let signature = context
        .session
        .load_function(module_id, function, type_arguments)
        .map_err(|e| context.convert_vm_error(e))?;
    let signature =
        subst_signature(signature, type_arguments).map_err(|e| context.convert_vm_error(e))?;
    let return_value_kinds = match function_kind {
        FunctionKind::Init => {
            assert_invariant!(
                signature.return_.is_empty(),
                "init functions must have no return values"
            );
            vec![]
        }
        FunctionKind::PrivateEntry | FunctionKind::PublicEntry | FunctionKind::NonEntry => {
            check_non_entry_signature::<_, Mode>(context, module_id, function, &signature)?
        }
    };
    check_private_generics(context, module_id, function, type_arguments)?;
    Ok(LoadedFunctionInfo {
        kind: function_kind,
        signature,
        return_value_kinds,
        index: FunctionDefinitionIndex(index as u16),
        last_instr,
    })
}

/// substitutes the type arguments into the parameter and return types
fn subst_signature(
    signature: LoadedFunctionInstantiation,
    type_arguments: &[Type],
) -> VMResult<LoadedFunctionInstantiation> {
    let LoadedFunctionInstantiation {
        parameters,
        return_,
    } = signature;
    let parameters = parameters
        .into_iter()
        .map(|ty| ty.subst(type_arguments))
        .collect::<PartialVMResult<Vec<_>>>()
        .map_err(|err| err.finish(Location::Undefined))?;
    let return_ = return_
        .into_iter()
        .map(|ty| ty.subst(type_arguments))
        .collect::<PartialVMResult<Vec<_>>>()
        .map_err(|err| err.finish(Location::Undefined))?;
    Ok(LoadedFunctionInstantiation {
        parameters,
        return_,
    })
}

/// Checks that the non-entry function does not return references. And marks the return values
/// as object or non-object return values
fn check_non_entry_signature<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    _module_id: &ModuleId,
    _function: &IdentStr,
    signature: &LoadedFunctionInstantiation,
) -> Result<Vec<ValueKind>, ExecutionError> {
    signature
        .return_
        .iter()
        .enumerate()
        .map(|(idx, return_type)| {
            let return_type = match return_type {
                // for dev-inspect, just dereference the value
                Type::Reference(inner) | Type::MutableReference(inner)
                    if Mode::allow_arbitrary_values() =>
                {
                    inner
                }
                Type::Reference(_) | Type::MutableReference(_) => {
                    return Err(ExecutionError::from_kind(
                        ExecutionErrorKind::InvalidPublicFunctionReturnType { idx: idx as u16 },
                    ))
                }
                t => t,
            };
            let abilities = context
                .session
                .get_type_abilities(return_type)
                .map_err(|e| context.convert_vm_error(e))?;
            Ok(match return_type {
                Type::MutableReference(_) | Type::Reference(_) => unreachable!(),
                Type::TyParam(_) => invariant_violation!("TyParam should have been substituted"),
                Type::Struct(_) | Type::StructInstantiation(_, _) if abilities.has_key() => {
                    let type_tag = context
                        .session
                        .get_type_tag(return_type)
                        .map_err(|e| context.convert_vm_error(e))?;
                    let TypeTag::Struct(struct_tag) = type_tag else {
                        invariant_violation!("Struct type make a non struct type tag")
                    };
                    ValueKind::Object {
                        type_: MoveObjectType::from(*struct_tag),
                        has_public_transfer: abilities.has_store(),
                    }
                }
                Type::Struct(_)
                | Type::StructInstantiation(_, _)
                | Type::Bool
                | Type::U8
                | Type::U64
                | Type::U128
                | Type::Address
                | Type::Signer
                | Type::Vector(_)
                | Type::U16
                | Type::U32
                | Type::U256 => ValueKind::Raw(return_type.clone(), abilities),
            })
        })
        .collect()
}

fn check_private_generics<S: StorageView>(
    _context: &mut ExecutionContext<S>,
    module_id: &ModuleId,
    function: &IdentStr,
    _type_arguments: &[Type],
) -> Result<(), ExecutionError> {
    let module_ident = (module_id.address(), module_id.name());
    if module_ident == (&SUI_FRAMEWORK_ADDRESS, EVENT_MODULE) {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            format!("Cannot directly call functions in sui::{}", EVENT_MODULE),
        ));
    }

    if module_ident == (&SUI_FRAMEWORK_ADDRESS, TRANSFER_MODULE)
        && PRIVATE_TRANSFER_FUNCTIONS.contains(&function)
    {
        let msg = format!(
            "Cannot directly call sui::{m}::{f}. \
            Use the public variant instead, sui::{m}::public_{f}",
            m = TRANSFER_MODULE,
            f = function
        );
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            msg,
        ));
    }

    Ok(())
}

type ArgInfo = (
    TxContextKind,
    /* mut ref */
    Vec<(LocalIndex, ValueKind)>,
    Vec<Vec<u8>>,
);

/// Serializes the arguments into BCS values for Move. Performs the necessary type checking for
/// each value
fn build_move_args<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    module_id: &ModuleId,
    function: &IdentStr,
    function_kind: FunctionKind,
    signature: &LoadedFunctionInstantiation,
    args: &[Argument],
) -> Result<ArgInfo, ExecutionError> {
    // check the arity
    let parameters = &signature.parameters;
    let tx_ctx_kind = match parameters.last() {
        Some(t) => is_tx_context(context, t)?,
        None => TxContextKind::None,
    };
    // an init function can have one or two arguments, with the last one always being of type
    // &mut TxContext and the additional (first) one representing a one time witness type (see
    // one_time_witness verifier pass for additional explanation)
    let has_one_time_witness = function_kind == FunctionKind::Init && parameters.len() == 2;
    let has_tx_context = tx_ctx_kind != TxContextKind::None;
    let num_args = args.len() + (has_one_time_witness as usize) + (has_tx_context as usize);
    if num_args != parameters.len() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::ArityMismatch,
            format!(
                "Expected {:?} argument{} calling function '{}', but found {:?}",
                parameters.len(),
                if parameters.len() == 1 { "" } else { "s" },
                function,
                num_args
            ),
        ));
    }

    // check the types and remember which are by mutable ref
    let mut by_mut_ref = vec![];
    let mut serialized_args = Vec::with_capacity(num_args);
    let command_kind = CommandKind::MoveCall {
        package: (*module_id.address()).into(),
        module: module_id.name(),
        function,
    };
    // an init function can have one or two arguments, with the last one always being of type
    // &mut TxContext and the additional (first) one representing a one time witness type (see
    // one_time_witness verifier pass for additional explanation)
    if has_one_time_witness {
        // one time witness type is a struct with a single bool filed which in bcs is encoded as
        // 0x01
        let bcs_true_value = bcs::to_bytes(&true).unwrap();
        serialized_args.push(bcs_true_value)
    }
    for ((idx, arg), param_ty) in args.iter().copied().enumerate().zip(parameters) {
        let (value, non_ref_param_ty): (Value, &Type) = match param_ty {
            Type::MutableReference(inner) => {
                let value = context.borrow_arg_mut(idx, arg)?;
                let object_info = if let Value::Object(ObjectValue {
                    type_,
                    has_public_transfer,
                    ..
                }) = &value
                {
                    let type_tag = context
                        .session
                        .get_type_tag(type_)
                        .map_err(|e| context.convert_vm_error(e))?;
                    let TypeTag::Struct(struct_tag) = type_tag else {
                        invariant_violation!("Struct type make a non struct type tag")
                    };
                    let type_ = (*struct_tag).into();
                    ValueKind::Object {
                        type_,
                        has_public_transfer: *has_public_transfer,
                    }
                } else {
                    let abilities = context
                        .session
                        .get_type_abilities(inner)
                        .map_err(|e| context.convert_vm_error(e))?;
                    ValueKind::Raw((**inner).clone(), abilities)
                };
                by_mut_ref.push((idx as LocalIndex, object_info));
                (value, inner)
            }
            Type::Reference(inner) => (context.borrow_arg(idx, arg)?, inner),
            t => {
                let value = context.by_value_arg(command_kind, idx, arg)?;
                (value, t)
            }
        };
        if matches!(
            function_kind,
            FunctionKind::PrivateEntry | FunctionKind::Init
        ) && value.was_used_in_non_entry_move_call()
        {
            return Err(command_argument_error(
                CommandArgumentError::InvalidArgumentToPrivateEntryFunction,
                idx,
            ));
        }
        check_param_type::<_, Mode>(context, idx, &value, non_ref_param_ty)?;
        let bytes = {
            let mut v = vec![];
            value.write_bcs_bytes(&mut v);
            v
        };
        serialized_args.push(bytes);
    }
    Ok((tx_ctx_kind, by_mut_ref, serialized_args))
}

/// checks that the value is compatible with the specified type
fn check_param_type<S: StorageView, Mode: ExecutionMode>(
    context: &mut ExecutionContext<S>,
    idx: usize,
    value: &Value,
    param_ty: &Type,
) -> Result<(), ExecutionError> {
    let ty = match value {
        // For dev-spect, allow any BCS bytes. This does mean internal invariants for types can
        // be violated (like for string or Option)
        Value::Raw(RawValueType::Any, _) if Mode::allow_arbitrary_values() => return Ok(()),
        // Any means this was just some bytes passed in as an argument (as opposed to being
        // generated from a Move function). Meaning we only allow "primitive" values
        // and might need to run validation in addition to the BCS layout
        Value::Raw(RawValueType::Any, bytes) => {
            let Some(layout) = primitive_serialization_layout(context, param_ty)? else {
                let msg = format!(
                    "Non-primitive argument at index {}. If it is an object, it must be \
                    populated by an object",
                    idx,
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::command_argument_error(
                        CommandArgumentError::InvalidUsageOfPureArg,
                        idx as u16,
                    ),
                    msg,
                ));
            };
            bcs_argument_validate(bytes, idx as u16, layout)?;
            return Ok(());
        }
        Value::Raw(RawValueType::Loaded { ty, abilities, .. }, _) => {
            assert_invariant!(
                Mode::allow_arbitrary_values() || !abilities.has_key(),
                "Raw value should never be an object"
            );
            ty
        }
        Value::Object(obj) => &obj.type_,
    };
    if ty != param_ty {
        Err(command_argument_error(
            CommandArgumentError::TypeMismatch,
            idx,
        ))
    } else {
        Ok(())
    }
}

fn get_struct_ident(s: &StructType) -> (&AccountAddress, &IdentStr, &IdentStr) {
    let module_id = &s.defining_id;
    let struct_name = &s.name;
    (
        module_id.address(),
        module_id.name(),
        struct_name.as_ident_str(),
    )
}

// Returns Some(kind) if the type is a reference to the TxnContext. kind being Mutable with
// a MutableReference, and Immutable otherwise.
// Returns None for all other types
pub fn is_tx_context<S: StorageView>(
    context: &mut ExecutionContext<S>,
    t: &Type,
) -> Result<TxContextKind, ExecutionError> {
    let (is_mut, inner) = match t {
        Type::MutableReference(inner) => (true, inner),
        Type::Reference(inner) => (false, inner),
        _ => return Ok(TxContextKind::None),
    };
    let Type::Struct(idx) = &**inner else { return Ok(TxContextKind::None) };
    let Some(s) = context.session.get_struct_type(*idx) else {
        invariant_violation!("Loaded struct not found")
    };
    let (module_addr, module_name, struct_name) = get_struct_ident(&s);
    let is_tx_context_type = module_addr == &SUI_FRAMEWORK_ADDRESS
        && module_name == TX_CONTEXT_MODULE_NAME
        && struct_name == TX_CONTEXT_STRUCT_NAME;
    Ok(if is_tx_context_type {
        if is_mut {
            TxContextKind::Mutable
        } else {
            TxContextKind::Immutable
        }
    } else {
        TxContextKind::None
    })
}

/// Returns Some(layout) iff it is a primitive, an ID, a String, or an option/vector of a valid type
fn primitive_serialization_layout<S: StorageView>(
    context: &mut ExecutionContext<S>,
    param_ty: &Type,
) -> Result<Option<PrimitiveArgumentLayout>, ExecutionError> {
    Ok(match param_ty {
        Type::Signer => return Ok(None),
        Type::Reference(_) | Type::MutableReference(_) | Type::TyParam(_) => {
            invariant_violation!("references and type parameters should be checked elsewhere")
        }
        Type::Bool => Some(PrimitiveArgumentLayout::Bool),
        Type::U8 => Some(PrimitiveArgumentLayout::U8),
        Type::U16 => Some(PrimitiveArgumentLayout::U16),
        Type::U32 => Some(PrimitiveArgumentLayout::U32),
        Type::U64 => Some(PrimitiveArgumentLayout::U64),
        Type::U128 => Some(PrimitiveArgumentLayout::U128),
        Type::U256 => Some(PrimitiveArgumentLayout::U256),
        Type::Address => Some(PrimitiveArgumentLayout::Address),

        Type::Vector(inner) => {
            let info_opt = primitive_serialization_layout(context, inner)?;
            info_opt.map(|layout| PrimitiveArgumentLayout::Vector(Box::new(layout)))
        }
        Type::StructInstantiation(idx, targs) => {
            let Some(s) = context.session.get_struct_type(*idx) else {
                invariant_violation!("Loaded struct not found")
            };
            let resolved_struct = get_struct_ident(&s);
            // is option of a string
            if resolved_struct == RESOLVED_STD_OPTION && targs.len() == 1 {
                let info_opt = primitive_serialization_layout(context, &targs[0])?;
                info_opt.map(|layout| PrimitiveArgumentLayout::Option(Box::new(layout)))
            } else {
                None
            }
        }
        Type::Struct(idx) => {
            let Some(s) = context.session.get_struct_type(*idx) else {
                invariant_violation!("Loaded struct not found")
            };
            let resolved_struct = get_struct_ident(&s);
            if resolved_struct == RESOLVED_SUI_ID {
                Some(PrimitiveArgumentLayout::Address)
            } else if resolved_struct == RESOLVED_ASCII_STR {
                Some(PrimitiveArgumentLayout::Ascii)
            } else if resolved_struct == RESOLVED_UTF8_STR {
                Some(PrimitiveArgumentLayout::UTF8)
            } else {
                None
            }
        }
    })
}

/***************************************************************************************************
 * Special serialization formats
 **************************************************************************************************/

/// Special enum for values that need additional validation, in other words
/// There is validation to do on top of the BCS layout. Currently only needed for
/// strings
#[derive(Debug)]
pub enum PrimitiveArgumentLayout {
    /// An option
    Option(Box<PrimitiveArgumentLayout>),
    /// A vector
    Vector(Box<PrimitiveArgumentLayout>),
    /// An ASCII encoded string
    Ascii,
    /// A UTF8 encoded string
    UTF8,
    // needed for Option validation
    Bool,
    U8,
    U16,
    U32,
    U64,
    U128,
    U256,
    Address,
}

impl PrimitiveArgumentLayout {
    /// returns true iff all BCS compatible bytes are actually values for this type.
    /// For example, this function returns false for Option and Strings since they need additional
    /// validation.
    pub fn bcs_only(&self) -> bool {
        match self {
            // have additional restrictions past BCS
            PrimitiveArgumentLayout::Option(_)
            | PrimitiveArgumentLayout::Ascii
            | PrimitiveArgumentLayout::UTF8 => false,
            // Move primitives are BCS compatible and do not need additional validation
            PrimitiveArgumentLayout::Bool
            | PrimitiveArgumentLayout::U8
            | PrimitiveArgumentLayout::U16
            | PrimitiveArgumentLayout::U32
            | PrimitiveArgumentLayout::U64
            | PrimitiveArgumentLayout::U128
            | PrimitiveArgumentLayout::U256
            | PrimitiveArgumentLayout::Address => true,
            // vector only needs validation if it's inner type does
            PrimitiveArgumentLayout::Vector(inner) => inner.bcs_only(),
        }
    }
}

/// Checks the bytes against the `SpecialArgumentLayout` using `bcs`. It does not actually generate
/// the deserialized value, only walks the bytes. While not necessary if the layout does not contain
/// special arguments (e.g. Option or String) we check the BCS bytes for predictability
pub fn bcs_argument_validate(
    bytes: &[u8],
    idx: u16,
    layout: PrimitiveArgumentLayout,
) -> Result<(), ExecutionError> {
    bcs::from_bytes_seed(&layout, bytes).map_err(|_| {
        ExecutionError::new_with_source(
            ExecutionErrorKind::command_argument_error(CommandArgumentError::InvalidBCSBytes, idx),
            format!("Function expects {layout} but provided argument's value does not match",),
        )
    })
}

impl<'d> serde::de::DeserializeSeed<'d> for &PrimitiveArgumentLayout {
    type Value = ();
    fn deserialize<D: serde::de::Deserializer<'d>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        use serde::de::Error;
        match self {
            PrimitiveArgumentLayout::Ascii => {
                let s: &str = serde::Deserialize::deserialize(deserializer)?;
                if !s.is_ascii() {
                    Err(D::Error::custom("not an ascii string"))
                } else {
                    Ok(())
                }
            }
            PrimitiveArgumentLayout::UTF8 => {
                deserializer.deserialize_string(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::Option(layout) => {
                deserializer.deserialize_option(OptionElementVisitor(layout))
            }
            PrimitiveArgumentLayout::Vector(layout) => {
                deserializer.deserialize_seq(VectorElementVisitor(layout))
            }
            // primitive move value cases, which are hit to make sure the correct number of bytes
            // are removed for elements of an option/vector
            PrimitiveArgumentLayout::Bool => {
                deserializer.deserialize_bool(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U8 => {
                deserializer.deserialize_u8(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U16 => {
                deserializer.deserialize_u16(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U32 => {
                deserializer.deserialize_u32(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U64 => {
                deserializer.deserialize_u64(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U128 => {
                deserializer.deserialize_u128(serde::de::IgnoredAny)?;
                Ok(())
            }
            PrimitiveArgumentLayout::U256 => {
                U256::deserialize(deserializer)?;
                Ok(())
            }
            PrimitiveArgumentLayout::Address => {
                SuiAddress::deserialize(deserializer)?;
                Ok(())
            }
        }
    }
}

struct VectorElementVisitor<'a>(&'a PrimitiveArgumentLayout);

impl<'d, 'a> serde::de::Visitor<'d> for VectorElementVisitor<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Vector")
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'d>,
    {
        while seq.next_element_seed(self.0)?.is_some() {}
        Ok(())
    }
}

struct OptionElementVisitor<'a>(&'a PrimitiveArgumentLayout);

impl<'d, 'a> serde::de::Visitor<'d> for OptionElementVisitor<'a> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("Option")
    }

    fn visit_none<E>(self) -> Result<Self::Value, E>
    where
        E: serde::de::Error,
    {
        Ok(())
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: serde::Deserializer<'d>,
    {
        self.0.deserialize(deserializer)
    }
}

impl fmt::Display for PrimitiveArgumentLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PrimitiveArgumentLayout::Vector(inner) => {
                write!(f, "vector<{inner}>")
            }
            PrimitiveArgumentLayout::Option(inner) => {
                write!(f, "std::option::Option<{inner}>")
            }
            PrimitiveArgumentLayout::Ascii => {
                write!(f, "std::{}::{}", RESOLVED_ASCII_STR.1, RESOLVED_ASCII_STR.2)
            }
            PrimitiveArgumentLayout::UTF8 => {
                write!(f, "std::{}::{}", RESOLVED_UTF8_STR.1, RESOLVED_UTF8_STR.2)
            }
            PrimitiveArgumentLayout::Bool => write!(f, "bool"),
            PrimitiveArgumentLayout::U8 => write!(f, "u8"),
            PrimitiveArgumentLayout::U16 => write!(f, "u16"),
            PrimitiveArgumentLayout::U32 => write!(f, "u32"),
            PrimitiveArgumentLayout::U64 => write!(f, "u64"),
            PrimitiveArgumentLayout::U128 => write!(f, "u128"),
            PrimitiveArgumentLayout::U256 => write!(f, "u256"),
            PrimitiveArgumentLayout::Address => write!(f, "address"),
        }
    }
}

}
