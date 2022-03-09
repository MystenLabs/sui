// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use crate::bytecode_rewriter::ModuleHandleRewriter;
use move_binary_format::{errors::PartialVMResult, file_format::CompiledModule};
use sui_framework::EventType;
use sui_types::{
    base_types::*,
    error::{SuiError, SuiResult},
    event::Event,
    gas,
    id::VersionedID,
    messages::ExecutionStatus,
    move_package::*,
    object::{MoveObject, Object, Owner},
    storage::{DeleteKind, Storage},
};
use sui_verifier::verifier;

use move_cli::sandbox::utils::get_gas_status;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, TypeTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::{native_functions::NativeFunctionTable, session::ExecutionResult};
use std::{
    borrow::Borrow,
    cmp,
    collections::{BTreeMap, HashMap, HashSet},
    convert::TryFrom,
    fmt::Debug,
    sync::Arc,
};

pub use move_vm_runtime::move_vm::MoveVM;

macro_rules! exec_failure {
    ($gas_used:expr, $err:expr) => {
        return Ok(ExecutionStatus::new_failure($gas_used, $err))
    };
}

#[cfg(test)]
#[path = "unit_tests/adapter_tests.rs"]
mod adapter_tests;

pub fn new_move_vm(natives: NativeFunctionTable) -> Result<Arc<MoveVM>, SuiError> {
    Ok(Arc::new(
        MoveVM::new(natives).map_err(|_| SuiError::ExecutionInvariantViolation)?,
    ))
}

/// Execute `module::function<type_args>(object_args ++ pure_args)` as a call from `sender` with the given `gas_budget`.
/// Execution will read from/write to the store in `state_view`.
/// IMPORTANT NOTES on the return value:
/// The return value indicates whether a system error has occurred (i.e. issues with the sui system, not with user transaction).
/// As long as there are no system issues we return Ok(ExecutionStatus).
/// ExecutionStatus indicates the execution result. If execution failed, we wrap both the gas used and the error
/// into ExecutionStatus::Failure.
#[allow(clippy::too_many_arguments)]
pub fn execute<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    vm: &MoveVM,
    state_view: &mut S,
    _natives: NativeFunctionTable,
    package_object: Object,
    module: &Identifier,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    object_args: Vec<Object>,
    pure_args: Vec<Vec<u8>>,
    gas_budget: u64,
    mut gas_object: Object,
    ctx: &mut TxContext,
) -> SuiResult<ExecutionStatus> {
    // object_owner_map maps from object ID to its exclusive object owner.
    // This map will be used for detecting circular ownership among
    // objects, which can only happen to objects exclusively owned
    // by objects.
    let mut object_owner_map = HashMap::new();
    for obj in &object_args {
        if let Owner::ObjectOwner(owner) = obj.owner {
            object_owner_map.insert(obj.id().into(), owner);
        }
    }
    let TypeCheckSuccess {
        module_id,
        args,
        mutable_ref_objects,
        by_value_objects,
    } = match resolve_and_type_check(
        package_object,
        module,
        function,
        &type_args,
        object_args,
        pure_args,
    ) {
        Ok(ok) => ok,
        Err(err) => {
            exec_failure!(gas::MIN_MOVE, err);
        }
    };

    let mut args = args;
    args.push(ctx.to_vec());
    match execute_internal(
        vm,
        state_view,
        &module_id,
        function,
        type_args,
        args,
        mutable_ref_objects,
        by_value_objects,
        object_owner_map,
        gas_budget,
        ctx,
        false,
    ) {
        ExecutionStatus::Failure { gas_used, error } => {
            exec_failure!(gas_used, *error)
        }
        ExecutionStatus::Success { gas_used } => {
            match gas::try_deduct_gas(&mut gas_object, gas_used) {
                Ok(()) => {
                    state_view.write_object(gas_object);
                    Ok(ExecutionStatus::Success { gas_used })
                }
                Err(err) => exec_failure!(gas_budget, err),
            }
        }
    }
}

/// This function calls into Move VM to execute a Move function
/// call. It returns gas that needs to be colleted for this particular
/// Move call both on successful and failed execution.
#[allow(clippy::too_many_arguments)]
fn execute_internal<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    vm: &MoveVM,
    state_view: &mut S,
    module_id: &ModuleId,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    mutable_ref_objects: Vec<Object>,
    by_value_objects: BTreeMap<ObjectID, Object>,
    object_owner_map: HashMap<SuiAddress, SuiAddress>,
    gas_budget: u64, // gas budget for the current call operation
    ctx: &mut TxContext,
    for_publish: bool,
) -> ExecutionStatus {
    // TODO: Update Move gas constants to reflect the gas fee on sui.
    let cost_table = &move_vm_types::gas_schedule::INITIAL_COST_SCHEDULE;
    let mut gas_status =
        match get_gas_status(cost_table, Some(gas_budget)).map_err(|e| SuiError::GasBudgetTooHigh {
            error: e.to_string(),
        }) {
            Ok(ok) => ok,
            Err(err) => return ExecutionStatus::new_failure(gas::MIN_MOVE, err),
        };
    let session = vm.new_session(state_view);
    match session.execute_function_for_effects(
        module_id,
        function,
        type_args,
        args,
        &mut gas_status,
    ) {
        ExecutionResult::Success {
            change_set,
            events,
            return_values,
            mut mutable_ref_values,
            gas_used,
        } => {
            // we already checked that the function had no return types in resolve_and_type_check--it should
            // also not return any values at runtime
            debug_assert!(return_values.is_empty());
            // Sui Move programs should never touch global state, so ChangeSet should be empty
            debug_assert!(change_set.accounts().is_empty());
            // Input ref parameters we put in should be the same number we get out, plus one for the &mut TxContext
            debug_assert!(mutable_ref_objects.len() + 1 == mutable_ref_values.len());
            debug_assert!(gas_used <= gas_budget);
            if for_publish {
                // When this function is used during publishing, it
                // may be executed several times, with objects being
                // created in the Move VM in each Move call. In such
                // case, we need to update TxContext value so that it
                // reflects what happened each time we call into the
                // Move VM (e.g. to account for the number of created
                // objects). We guard it with a flag to avoid
                // serialization cost for non-publishing calls.
                let ctx_bytes = mutable_ref_values.pop().unwrap();
                let updated_ctx: TxContext = bcs::from_bytes(ctx_bytes.as_slice()).unwrap();
                if let Err(err) = ctx.update_state(updated_ctx) {
                    return ExecutionStatus::new_failure(gas_used, err);
                }
            }

            let mutable_refs = mutable_ref_objects
                .into_iter()
                .zip(mutable_ref_values.into_iter());
            let (extra_gas_used, gas_refund, result) = process_successful_execution(
                state_view,
                by_value_objects,
                mutable_refs,
                events,
                ctx,
                object_owner_map,
            );
            let total_gas = gas::aggregate_gas(gas_used + extra_gas_used, gas_refund);
            if let Err(err) = result {
                // Cap total_gas by gas_budget in the fail case.
                return ExecutionStatus::new_failure(cmp::min(total_gas, gas_budget), err);
            }
            // gas_budget should be enough to pay not only the VM execution cost,
            // but also the cost to process all events, such as transfers.
            if total_gas > gas_budget {
                ExecutionStatus::new_failure(
                    gas_budget,
                    SuiError::InsufficientGas {
                        error: format!(
                            "Total gas used ({}) exceeds gas budget ({})",
                            total_gas, gas_budget
                        ),
                    },
                )
            } else {
                ExecutionStatus::Success {
                    gas_used: total_gas,
                }
            }
        }
        // charge for all computations so far
        ExecutionResult::Fail { error, gas_used } => ExecutionStatus::new_failure(
            gas_used,
            SuiError::AbortedExecution {
                error: error.to_string(),
            },
        ),
    }
}

/// Similar to execute(), only returns Err if there are system issues.
/// ExecutionStatus contains the actual execution result.
pub fn publish<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    module_bytes: Vec<Vec<u8>>,
    ctx: &mut TxContext,
    gas_budget: u64,
    mut gas_object: Object,
) -> SuiResult<ExecutionStatus> {
    let result = module_bytes
        .iter()
        .map(|b| CompiledModule::deserialize(b))
        .collect::<PartialVMResult<Vec<CompiledModule>>>();
    let mut modules = match result {
        Ok(ok) => ok,
        Err(err) => {
            exec_failure!(
                gas::MIN_MOVE,
                SuiError::ModuleDeserializationFailure {
                    error: err.to_string(),
                }
            );
        }
    };

    // run validation checks
    let gas_used_for_publish = gas::calculate_module_publish_cost(&module_bytes);
    if gas_used_for_publish > gas_budget {
        exec_failure!(
            gas::MIN_MOVE,
            SuiError::InsufficientGas {
                error: "Gas budget insufficient to publish a package".to_string(),
            }
        );
    }
    if modules.is_empty() {
        exec_failure!(
            gas::MIN_MOVE,
            SuiError::ModulePublishFailure {
                error: "Publishing empty list of modules".to_string(),
            }
        );
    }

    let package_id = match generate_package_id(&mut modules, ctx) {
        Ok(ok) => ok,
        Err(err) => exec_failure!(gas::MIN_MOVE, err),
    };
    let vm = match verify_and_link(state_view, &modules, package_id, natives) {
        Ok(ok) => ok,
        Err(err) => exec_failure!(gas::MIN_MOVE, err),
    };

    let gas_used_for_init = match store_package_and_init_modules(
        state_view,
        &vm,
        modules,
        ctx,
        gas_budget - gas_used_for_publish,
    ) {
        ExecutionStatus::Success { gas_used } => gas_used,
        ExecutionStatus::Failure { gas_used, error } => {
            exec_failure!(gas_used + gas_used_for_publish, *error)
        }
    };

    let total_gas_used = gas_used_for_publish + gas_used_for_init;
    // successful execution of both publishing operation and or all
    // (optional) initializer calls
    match gas::try_deduct_gas(&mut gas_object, total_gas_used) {
        Ok(()) => {
            state_view.write_object(gas_object);
            Ok(ExecutionStatus::Success {
                gas_used: total_gas_used,
            })
        }
        Err(err) => exec_failure!(gas_budget, err),
    }
}

/// Store package in state_view and call module initializers
/// Return gas used for initialization
pub fn store_package_and_init_modules<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    state_view: &mut S,
    vm: &MoveVM,
    modules: Vec<CompiledModule>,
    ctx: &mut TxContext,
    gas_budget: u64,
) -> ExecutionStatus {
    let mut modules_to_init = Vec::new();
    for module in modules.iter() {
        if module_has_init(module) {
            modules_to_init.push(module.self_id());
        }
    }

    // wrap the modules in an object, write it to the store
    // The call to unwrap() will go away once we remove address owner from Immutable objects.
    let package_object = Object::new_package(modules, ctx.digest());
    state_view.write_object(package_object);

    init_modules(state_view, vm, modules_to_init, ctx, gas_budget)
}

/// Modules in module_ids_to_init must have the init method defined
fn init_modules<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    vm: &MoveVM,
    module_ids_to_init: Vec<ModuleId>,
    ctx: &mut TxContext,
    gas_budget: u64,
) -> ExecutionStatus {
    let mut total_gas_used = 0;
    let mut current_gas_budget = gas_budget;
    for module_id in module_ids_to_init {
        let args = vec![ctx.to_vec()];

        let gas_used = match execute_internal(
            vm,
            state_view,
            &module_id,
            &Identifier::new(INIT_FN_NAME.as_str()).unwrap(),
            Vec::new(),
            args,
            Vec::new(),
            BTreeMap::new(),
            HashMap::new(),
            current_gas_budget,
            ctx,
            true,
        ) {
            ExecutionStatus::Success { gas_used } => gas_used,
            ExecutionStatus::Failure { gas_used, error } => {
                return ExecutionStatus::Failure {
                    gas_used: gas_used + total_gas_used,
                    error,
                };
            }
        };
        // This should never be the case as current_gas_budget
        // (before the call) must be larger than gas_used (after
        // the call) in order for the call to succeed in the first
        // place.
        debug_assert!(current_gas_budget >= gas_used);
        current_gas_budget -= gas_used;
        total_gas_used += gas_used;
    }

    ExecutionStatus::Success {
        gas_used: total_gas_used,
    }
}

/// Given a list of `modules`, links each module against its
/// dependencies and runs each module with both the Move VM verifier
/// and the Sui verifier.
pub fn verify_and_link<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    state_view: &S,
    modules: &[CompiledModule],
    package_id: ObjectID,
    natives: NativeFunctionTable,
) -> Result<MoveVM, SuiError> {
    // Run the Move bytecode verifier and linker.
    // It is important to do this before running the Sui verifier, since the sui
    // verifier may assume well-formedness conditions enforced by the Move verifier hold
    let vm = MoveVM::new(natives)
        .expect("VM creation only fails if natives are invalid, and we created the natives");
    // Note: VM does not do any gas metering on publish code path, so setting budget to None is fine
    let cost_table = &move_vm_types::gas_schedule::INITIAL_COST_SCHEDULE;
    let mut gas_status = get_gas_status(cost_table, None)
        .expect("Can only fail if gas budget is too high, and we didn't supply one");
    let mut session = vm.new_session(state_view);
    // TODO(https://github.com/MystenLabs/sui/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
    let new_module_bytes = modules
        .iter()
        .map(|m| {
            let mut bytes = Vec::new();
            m.serialize(&mut bytes).unwrap();
            bytes
        })
        .collect();
    session
        .publish_module_bundle(
            new_module_bytes,
            AccountAddress::from(package_id),
            &mut gas_status,
        )
        .map_err(|e| SuiError::ModulePublishFailure {
            error: e.to_string(),
        })?;

    // run the Sui verifier
    for module in modules.iter() {
        // Run Sui bytecode verifier, which runs some additional checks that assume the Move bytecode verifier has passed.
        verifier::verify_module(module)?;
    }
    Ok(vm)
}

/// Given a list of `modules`, use `ctx` to generate a fresh ID for the new packages.
/// If `is_framework` is true, then the modules can have arbitrary user-defined address,
/// otherwise their addresses must be 0.
/// Mutate each module's self ID to the appropriate fresh ID and update its module handle tables
/// to reflect the new ID's of its dependencies.
/// Returns the newly created package ID.
pub fn generate_package_id(
    modules: &mut [CompiledModule],
    ctx: &mut TxContext,
) -> Result<ObjectID, SuiError> {
    let mut sub_map = BTreeMap::new();
    let package_id = ctx.fresh_id();
    for module in modules.iter() {
        let old_module_id = module.self_id();
        let old_address = *old_module_id.address();
        if old_address != AccountAddress::ZERO {
            return Err(SuiError::ModulePublishFailure {
                error: "Publishing modules with non-zero address is not allowed".to_string(),
            });
        }
        let new_module_id = ModuleId::new(
            AccountAddress::from(package_id),
            old_module_id.name().to_owned(),
        );
        if sub_map.insert(old_module_id, new_module_id).is_some() {
            return Err(SuiError::ModulePublishFailure {
                error: "Publishing two modules with the same ID".to_string(),
            });
        }
    }

    // Safe to unwrap because we checked for duplicate domain entries above, and range entries are fresh ID's
    let rewriter = ModuleHandleRewriter::new(sub_map).unwrap();
    for module in modules.iter_mut() {
        // rewrite module handles to reflect freshly generated ID's
        rewriter.sub_module_ids(module);
    }
    Ok(package_id)
}

type MoveEvent = (Vec<u8>, u64, TypeTag, Vec<u8>);

/// Update `state_view` with the effects of successfully executing a transaction:
/// - Look for each input in `by_value_objects` to determine whether the object was transferred, frozen, or deleted
/// - Update objects passed via a mutable reference in `mutable_refs` to their new values
/// - Process creation of new objects and user-emittd events in `events`
/// - Returns (amount of extra gas used, amount of gas refund, process result)
#[allow(clippy::too_many_arguments)]
fn process_successful_execution<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    state_view: &mut S,
    mut by_value_objects: BTreeMap<ObjectID, Object>,
    mutable_refs: impl Iterator<Item = (Object, Vec<u8>)>,
    events: Vec<MoveEvent>,
    ctx: &TxContext,
    mut object_owner_map: HashMap<SuiAddress, SuiAddress>,
) -> (u64, u64, SuiResult) {
    for (mut obj, new_contents) in mutable_refs {
        // update contents and increment sequence number
        obj.data
            .try_as_move_mut()
            .expect("We previously checked that mutable ref inputs are Move objects")
            .update_contents(new_contents);
        state_view.write_object(obj);
    }
    // process events to identify transfers, freezes
    let mut gas_used = 0;
    let tx_digest = ctx.digest();
    let mut deleted_ids = HashMap::new();
    let mut deleted_child_ids = HashSet::new();
    for e in events {
        let (recipient, event_type, type_, event_bytes) = e;
        let result = match EventType::try_from(event_type as u8)
            .expect("Safe because event_type is derived from an EventType enum")
        {
            EventType::TransferToAddress => handle_transfer(
                Owner::AddressOwner(SuiAddress::try_from(recipient.as_slice()).unwrap()),
                type_,
                event_bytes,
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::FreezeObject => handle_transfer(
                Owner::SharedImmutable,
                type_,
                event_bytes,
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::TransferToObject => handle_transfer(
                Owner::ObjectOwner(ObjectID::try_from(recipient.borrow()).unwrap().into()),
                type_,
                event_bytes,
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::DeleteObjectID => {
                // unwrap safe because this event can only be emitted from processing
                // native call delete_id, which guarantees the type of the id.
                let id: VersionedID = bcs::from_bytes(&event_bytes).unwrap();
                deleted_ids.insert(*id.object_id(), id.version());
                Ok(())
            }
            EventType::ShareObject => Err(SuiError::UnsupportedSharedObjectError),
            EventType::DeleteChildObject => {
                match type_ {
                    TypeTag::Struct(s) => {
                        let obj = MoveObject::new(s, event_bytes);
                        deleted_ids.insert(obj.id(), obj.version());
                        deleted_child_ids.insert(obj.id());
                    },
                    _ => unreachable!(
                        "Native function delete_child_object_internal<T> ensures that T is always bound to structs"
                    ),
                }
                Ok(())
            }
            EventType::User => {
                match type_ {
                    TypeTag::Struct(s) => state_view.log_event(Event::new(s, event_bytes)),
                    _ => unreachable!(
                        "Native function emit_event<T> ensures that T is always bound to structs"
                    ),
                };
                Ok(())
            }
        };
        if result.is_err() {
            return (gas_used, 0, result);
        }
    }

    // any object left in `by_value_objects` is an input passed by value that was not transferred or frozen.
    // this means that either the object was (1) deleted from the Sui system altogether, or
    // (2) wrapped inside another object that is in the Sui object pool
    let mut gas_refund: u64 = 0;
    for (id, object) in by_value_objects.iter() {
        // If an object is owned by another object, we are not allowed to directly delete the child
        // object because this could lead to a dangling reference of the ownership. Such
        // dangling reference can never be dropped. To delete this object, one must either first transfer
        // the child object to an account address, or call through Transfer::delete_child_object(),
        // which would consume both the child object and the ChildRef ownership reference,
        // and emit the DeleteChildObject event. These child objects can be safely deleted.
        if matches!(object.owner, Owner::ObjectOwner { .. }) && !deleted_child_ids.contains(id) {
            return (gas_used, 0, Err(SuiError::DeleteObjectOwnedObject));
        }
        if deleted_ids.contains_key(id) {
            state_view.delete_object(id, object.version(), DeleteKind::ExistInInput);
        } else {
            state_view.delete_object(id, object.version(), DeleteKind::Wrap);
        }

        gas_refund += gas::calculate_object_deletion_refund(object);
    }
    // The loop above may not cover all ids in deleted_ids, i.e. some of the deleted_ids
    // may not show up in by_value_objects.
    // This can happen for two reasons:
    //  1. The call to ID::delete_id() was not a result of deleting a pre-existing object.
    //    This can happen either because we were deleting an object that just got created
    //    in this same transaction; or we have an ID that's created but not associated with
    //    a real object. In either case, we don't care about this id.
    //  2. This object was wrapped in the past, and now is getting deleted. It won't show up
    //    in the input, but the deletion is also real.
    // We cannot distinguish the above two cases here just yet. So we just add it with
    // the kind NotExistInInput. They will be eventually filtered out in
    // [`AuthorityTemporaryStore::patch_unwrapped_objects`].
    for (id, version) in deleted_ids {
        if !by_value_objects.contains_key(&id) {
            state_view.delete_object(&id, version, DeleteKind::NotExistInInput);
        }
    }

    (gas_used, gas_refund, Ok(()))
}

#[allow(clippy::too_many_arguments)]
fn handle_transfer<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    recipient: Owner,
    type_: TypeTag,
    contents: Vec<u8>,
    tx_digest: TransactionDigest,
    by_value_objects: &mut BTreeMap<ObjectID, Object>,
    gas_used: &mut u64,
    state_view: &mut S,
    object_owner_map: &mut HashMap<SuiAddress, SuiAddress>,
) -> SuiResult {
    match type_ {
        TypeTag::Struct(s_type) => {
            let mut move_obj = MoveObject::new(s_type, contents);
            let old_object = by_value_objects.remove(&move_obj.id());

            #[cfg(debug_assertions)]
            {
                check_transferred_object_invariants(&move_obj, &old_object)
            }

            // increment the object version. note that if the transferred object was
            // freshly created, this means that its version will now be 1.
            // thus, all objects in the global object pool have version > 0
            move_obj.increment_version();
            let obj = Object::new_move(move_obj, recipient, tx_digest);
            if old_object.is_none() {
                // Charge extra gas based on object size if we are creating a new object.
                *gas_used += gas::calculate_object_creation_cost(&obj);
            }
            let obj_address: SuiAddress = obj.id().into();
            object_owner_map.remove(&obj_address);
            if let Owner::ObjectOwner(new_owner) = recipient {
                // Below we check whether the transfer introduced any circular ownership.
                // We know that for any mutable object, all its ancenstors (if it was owned by another object)
                // must be in the input as well. Prior to this we have recorded the original ownership mapping
                // in object_owner_map. For any new transfer, we trace the new owner through the ownership
                // chain to see if a cycle is detected.
                // TODO: Set a constant upper bound to the depth of the new ownership chain.
                let mut parent = new_owner;
                while parent != obj_address && object_owner_map.contains_key(&parent) {
                    parent = *object_owner_map.get(&parent).unwrap();
                }
                if parent == obj_address {
                    return Err(SuiError::CircularObjectOwnership);
                }
                object_owner_map.insert(obj_address, new_owner);
            }

            state_view.write_object(obj);
        }
        _ => unreachable!("Only structs can be transferred"),
    }
    Ok(())
}

#[cfg(debug_assertions)]
fn check_transferred_object_invariants(new_object: &MoveObject, old_object: &Option<Object>) {
    if let Some(o) = old_object {
        // check consistency between the transferred object `new_object` and the tx input `o`
        // specifically, the object id, type, and version should be unchanged
        let m = o.data.try_as_move().unwrap();
        debug_assert_eq!(m.id(), new_object.id());
        debug_assert_eq!(m.version(), new_object.version());
        debug_assert_eq!(m.type_, new_object.type_);
    }
}
