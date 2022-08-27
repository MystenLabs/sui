// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    convert::TryFrom,
    fmt::Debug,
};

use anyhow::Result;
use leb128;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    file_format::{AbilitySet, CompiledModule, LocalIndex, SignatureToken, StructHandleIndex},
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    resolver::{ModuleResolver, ResourceResolver},
};
pub use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::{native_functions::NativeFunctionTable, session::SerializedReturnValues};

use sui_framework::EventType;
use sui_types::{
    base_types::*,
    error::ExecutionError,
    error::{ExecutionErrorKind, SuiError},
    event::{Event, TransferType},
    gas::SuiGasStatus,
    id::UID,
    messages::{CallArg, EntryArgumentErrorKind, InputObjectKind, ObjectArg},
    object::{self, Data, MoveObject, Object, Owner, ID_END_INDEX},
    storage::{DeleteKind, ObjectChange, ParentSync, Storage},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_verifier::{
    entry_points_verifier::{is_tx_context, RESOLVED_STD_OPTION, RESOLVED_SUI_ID},
    verifier, INIT_FN_NAME,
};

use crate::bytecode_rewriter::ModuleHandleRewriter;
use crate::object_root_ancestor_map::ObjectRootAncestorMap;

pub fn new_move_vm(natives: NativeFunctionTable) -> Result<MoveVM, SuiError> {
    MoveVM::new(natives).map_err(|_| SuiError::ExecutionInvariantViolation)
}

/// Execute `module::function<type_args>(object_args ++ pure_args)` as a call from `sender` with the given `gas_budget`.
/// Execution will read from/write to the store in `state_view`.
/// IMPORTANT NOTES on the return value:
/// The return value is a two-layer SuiResult. The outer layer indicates whether a system error
/// has occurred (i.e. issues with the sui system, not with user transaction).
/// As long as there are no system issues we return Ok(SuiResult).
/// The inner SuiResult indicates the execution result. If execution failed, we return Ok(Err),
/// otherwise we return Ok(Ok).
/// TODO: Do we really need the two layers?
#[allow(clippy::too_many_arguments)]
pub fn execute<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    vm: &MoveVM,
    state_view: &mut S,
    module_id: ModuleId,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    args: Vec<CallArg>,
    gas_status: &mut SuiGasStatus,
    ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    let objects = args
        .iter()
        .filter_map(|arg| match arg {
            CallArg::Pure(_) => None,
            CallArg::Object(ObjectArg::ImmOrOwnedObject((id, _, _)))
            | CallArg::Object(ObjectArg::SharedObject(id)) => {
                Some(vec![(*id, state_view.read_object(id)?)])
            }
            CallArg::ObjVec(vec) => {
                if vec.is_empty() {
                    return None;
                }
                Some(
                    vec.iter()
                        .filter_map(|obj_arg| match obj_arg {
                            ObjectArg::ImmOrOwnedObject((id, _, _)) => {
                                Some((*id, state_view.read_object(id)?))
                            }
                            ObjectArg::SharedObject(_) => {
                                // ObjVec is guaranteed to never contain shared objects
                                debug_assert!(false);
                                None
                            }
                        })
                        .collect(),
                )
            }
        })
        .flatten()
        .collect();
    let module = vm.load_module(&module_id, state_view)?;
    let is_genesis = ctx.digest() == TransactionDigest::genesis();
    let TypeCheckSuccess {
        module_id,
        mut args,
        object_data,
        by_value_objects,
        mutable_ref_objects,
        has_ctx_arg,
    } = resolve_and_type_check(&objects, &module, function, &type_args, args, is_genesis)?;

    if has_ctx_arg {
        args.push(ctx.to_vec());
    }
    execute_internal(
        vm,
        state_view,
        &module_id,
        function,
        type_args,
        args,
        has_ctx_arg,
        object_data,
        by_value_objects,
        mutable_ref_objects,
        gas_status,
        ctx,
    )
}

/// This function calls into Move VM to execute a Move function
/// call.
#[allow(clippy::too_many_arguments)]
fn execute_internal<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    vm: &MoveVM,
    state_view: &mut S,
    module_id: &ModuleId,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    has_ctx_arg: bool,
    object_data: BTreeMap<ObjectID, (object::Owner, SequenceNumber, Option<u32>)>,
    by_value_objects: BTreeSet<ObjectID>,
    mut mutable_ref_objects: BTreeMap<LocalIndex, ObjectID>,
    gas_status: &mut SuiGasStatus, // gas status for the current call operation
    ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    // object_owner_map maps from object ID to its exclusive object owner.
    // This map will be used for detecting circular ownership among
    // objects, which can only happen to objects exclusively owned
    // by objects.
    let object_owner_map: BTreeMap<SuiAddress, SuiAddress> = object_data
        .iter()
        .filter_map(|(id, (owner, _, _))| match owner {
            Owner::ObjectOwner(owner) => Some(((*id).into(), *owner)),
            _ => None,
        })
        .collect();

    let mut session = vm.new_session(state_view);
    // script visibility checked manually for entry points
    let (
        SerializedReturnValues {
            mut mutable_reference_outputs,
            return_values,
        },
        (change_set, events),
    ) = session
        .execute_function_bypass_visibility(
            module_id,
            function,
            type_args,
            args,
            gas_status.get_move_gas_status(),
        )
        .and_then(|ret| Ok((ret, session.finish()?)))?;

    // Sui Move programs should never touch global state, so ChangeSet should be empty
    debug_assert!(change_set.accounts().is_empty());
    // Input ref parameters we put in should be the same number we get out, plus one for the &mut TxContext
    let num_mut_objects = if has_ctx_arg {
        mutable_ref_objects.len() + 1
    } else {
        mutable_ref_objects.len()
    };
    debug_assert!(num_mut_objects == mutable_reference_outputs.len());

    // When this function is used during publishing, it
    // may be executed several times, with objects being
    // created in the Move VM in each Move call. In such
    // case, we need to update TxContext value so that it
    // reflects what happened each time we call into the
    // Move VM (e.g. to account for the number of created
    // objects).
    if has_ctx_arg {
        let (_, ctx_bytes, _) = mutable_reference_outputs.pop().unwrap();
        let updated_ctx: TxContext = bcs::from_bytes(&ctx_bytes).unwrap();
        ctx.update_state(updated_ctx)?;
    }

    let mutable_refs = mutable_reference_outputs
        .into_iter()
        .map(|(local_idx, bytes, _layout)| {
            let object_id = mutable_ref_objects.remove(&local_idx).unwrap();
            debug_assert!(!by_value_objects.contains(&object_id));
            (object_id, bytes)
        })
        .collect();
    // All mutable references should have been marked as updated
    debug_assert!(mutable_ref_objects.is_empty());
    let by_value_object_map = object_data
        .into_iter()
        .filter(|(id, _obj)| by_value_objects.contains(id))
        .collect();
    let session = vm.new_session(state_view);
    let events = events
        .into_iter()
        .map(|(recipient, event_type, type_, event_bytes)| {
            let loaded_type = session.load_type(&type_)?;
            let abilities = session.get_type_abilities(&loaded_type)?;
            Ok((recipient, event_type, type_, abilities, event_bytes))
        })
        .collect::<Result<_, ExecutionError>>()?;
    let (empty_changes, empty_events) = session.finish()?;
    debug_assert!(empty_changes.into_inner().is_empty());
    debug_assert!(empty_events.is_empty());
    process_successful_execution(
        state_view,
        module_id,
        by_value_object_map,
        mutable_refs,
        events,
        ctx,
        object_owner_map,
    )?;

    debug_assert!(return_values.is_empty());
    Ok(())
}

pub fn publish<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    module_bytes: Vec<Vec<u8>>,
    ctx: &mut TxContext,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    gas_status.charge_publish_package(module_bytes.iter().map(|v| v.len()).sum())?;
    let mut modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize(b)
                .map_err(|e| e.finish(move_binary_format::errors::Location::Undefined))
        })
        .collect::<move_binary_format::errors::VMResult<Vec<CompiledModule>>>()?;

    if modules.is_empty() {
        return Err(ExecutionErrorKind::PublishErrorEmptyPackage.into());
    }

    let package_id = generate_package_id(&mut modules, ctx)?;
    let vm = verify_and_link(state_view, &modules, package_id, natives, gas_status)?;
    state_view.log_event(Event::Publish {
        sender: ctx.sender(),
        package_id,
    });
    store_package_and_init_modules(state_view, &vm, modules, ctx, gas_status)
}

/// Store package in state_view and call module initializers
pub fn store_package_and_init_modules<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    state_view: &mut S,
    vm: &MoveVM,
    modules: Vec<CompiledModule>,
    ctx: &mut TxContext,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    let modules_to_init = modules
        .iter()
        .filter_map(|module| {
            for fdef in &module.function_defs {
                let fhandle = module.function_handle_at(fdef.function);
                let fname = module.identifier_at(fhandle.name);
                if fname == INIT_FN_NAME {
                    return Some((
                        module.self_id(),
                        module.signature_at(fhandle.parameters).len(),
                    ));
                }
            }
            None
        })
        .collect();

    // wrap the modules in an object, write it to the store
    // The call to unwrap() will go away once we remove address owner from Immutable objects.
    let package_object = Object::new_package(modules, ctx.digest());
    let id = package_object.id();
    state_view.set_create_object_ids(BTreeSet::from([id]));
    let changes = BTreeMap::from([(id, ObjectChange::Write(package_object))]);
    state_view.apply_object_changes(changes);

    init_modules(state_view, vm, modules_to_init, ctx, gas_status)
}

/// Modules in module_ids_to_init must have the init method defined
fn init_modules<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    state_view: &mut S,
    vm: &MoveVM,
    module_ids_to_init: Vec<(ModuleId, usize)>,
    ctx: &mut TxContext,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    let init_ident = Identifier::new(INIT_FN_NAME.as_str()).unwrap();
    for (module_id, num_args) in module_ids_to_init {
        let mut args = vec![];
        // an init function can have one or two arguments, with the last one always being of type
        // &mut TxContext and the additional (first) one representing a characteristic type (see
        // char_type verfier pass for additional explanation)
        if num_args == 2 {
            // characteristic type is a struct with a single bool filed which in bcs is encoded as
            // 0x01
            let bcs_char_type_value = vec![0x01];
            args.push(bcs_char_type_value);
        }
        args.push(ctx.to_vec());
        let has_ctx_arg = true;

        execute_internal(
            vm,
            state_view,
            &module_id,
            &init_ident,
            Vec::new(),
            args,
            has_ctx_arg,
            BTreeMap::new(),
            BTreeSet::new(),
            BTreeMap::new(),
            gas_status,
            ctx,
        )?;
    }
    Ok(())
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
    gas_status: &mut SuiGasStatus,
) -> Result<MoveVM, ExecutionError> {
    // Run the Move bytecode verifier and linker.
    // It is important to do this before running the Sui verifier, since the sui
    // verifier may assume well-formedness conditions enforced by the Move verifier hold
    let vm = MoveVM::new(natives)
        .expect("VM creation only fails if natives are invalid, and we created the natives");
    let mut session = vm.new_session(state_view);
    // TODO(https://github.com/MystenLabs/sui/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
    let new_module_bytes: Vec<_> = modules
        .iter()
        .map(|m| {
            let mut bytes = Vec::new();
            m.serialize(&mut bytes).unwrap();
            bytes
        })
        .collect();
    session.publish_module_bundle(
        new_module_bytes,
        AccountAddress::from(package_id),
        // TODO: publish_module_bundle() currently doesn't charge gas.
        // Do we want to charge there?
        gas_status.get_move_gas_status(),
    )?;

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
) -> Result<ObjectID, ExecutionError> {
    let mut sub_map = BTreeMap::new();
    let package_id = ctx.fresh_id();
    for module in modules.iter() {
        let old_module_id = module.self_id();
        let old_address = *old_module_id.address();
        if old_address != AccountAddress::ZERO {
            let handle = module.module_handle_at(module.self_module_handle_idx);
            let name = module.identifier_at(handle.name);
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorNonZeroAddress,
                format!("Publishing module {name} with non-zero address is not allowed"),
            ));
        }
        let new_module_id = ModuleId::new(
            AccountAddress::from(package_id),
            old_module_id.name().to_owned(),
        );
        if sub_map.insert(old_module_id, new_module_id).is_some() {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::PublishErrorDuplicateModule,
                "Publishing two modules with the same ID",
            ));
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

type MoveEvent = (Vec<u8>, u64, TypeTag, AbilitySet, Vec<u8>);

/// Update `state_view` with the effects of successfully executing a transaction:
/// - Look for each input in `by_value_objects` to determine whether the object was transferred, frozen, or deleted
/// - Update objects passed via a mutable reference in `mutable_refs` to their new values
/// - Process creation of new objects and user-emittd events in `events`
#[allow(clippy::too_many_arguments)]
fn process_successful_execution<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    state_view: &mut S,
    module_id: &ModuleId,
    mut by_value_objects: BTreeMap<ObjectID, (object::Owner, SequenceNumber, Option<u32>)>,
    mutable_refs: Vec<(ObjectID, Vec<u8>)>,
    events: Vec<MoveEvent>,
    ctx: &TxContext,
    mut object_owner_map: BTreeMap<SuiAddress, SuiAddress>,
) -> Result<(), ExecutionError> {
    let mut changes = BTreeMap::new();
    for (obj_id, new_contents) in mutable_refs {
        // update contents and increment sequence number
        let mut obj = state_view
            .read_object(&obj_id)
            .expect("We previously checked all input objects exist")
            .clone();
        obj.data
            .try_as_move_mut()
            .expect("We previously checked that mutable ref inputs are Move objects")
            .update_contents_and_increment_version(new_contents);
        changes.insert(obj_id, ObjectChange::Write(obj));
    }
    let tx_digest = ctx.digest();
    // newly_generated_ids contains all object IDs generated in this transaction.
    // TODO: In the case of the special system transaction that creates the system state object,
    // there is one extra object created with ID hardcoded at 0x5, and it won't be included in
    // `newly_generated_ids`. To cope with this, we special check the ID inside `handle_transfer`.
    // It's a bit hacky. Ideally, we want `newly_generated_ids` to include it. But it's unclear how.
    let mut newly_generated_ids = ctx.recreate_all_ids();
    let mut frozen_object_ids = BTreeSet::new();
    let mut child_count_deltas: BTreeMap<ObjectID, i64> = BTreeMap::new();
    let mut newly_generated_deleted = BTreeSet::new();
    let mut newly_generated_unused = newly_generated_ids.clone();
    // process events to identify transfers, freezes
    for e in events {
        let (recipient, event_type, type_, abilities, event_bytes) = e;
        let event_type = EventType::try_from(event_type as u8)
            .expect("Safe because event_type is derived from an EventType enum");
        match event_type {
            EventType::TransferToAddress
            | EventType::FreezeObject
            | EventType::TransferToObject
            | EventType::ShareObject => {
                let new_owner = match event_type {
                    EventType::TransferToAddress => {
                        Owner::AddressOwner(SuiAddress::try_from(recipient.as_slice()).unwrap())
                    }
                    EventType::FreezeObject => Owner::Immutable,
                    EventType::TransferToObject => {
                        Owner::ObjectOwner(ObjectID::try_from(recipient.borrow()).unwrap().into())
                    }
                    EventType::ShareObject => Owner::Shared,
                    _ => unreachable!(),
                };
                let obj = handle_transfer(
                    ctx.sender(),
                    new_owner,
                    type_,
                    abilities,
                    event_bytes,
                    tx_digest,
                    &mut by_value_objects,
                    state_view,
                    module_id,
                    &mut object_owner_map,
                    &mut newly_generated_ids,
                    &mut newly_generated_unused,
                    &mut frozen_object_ids,
                    &mut child_count_deltas,
                )?;
                changes.insert(obj.id(), ObjectChange::Write(obj));
            }
            EventType::DeleteObjectID => {
                // unwrap safe because this event can only be emitted from processing
                // native call delete_id, which guarantees the type of the id.
                let uid: UID = bcs::from_bytes(&event_bytes).unwrap();
                let obj_id = uid.object_id();
                newly_generated_unused.remove(obj_id);
                if newly_generated_ids.contains(obj_id) {
                    // we will need to make sure that this deleted object did not receive any
                    // children
                    newly_generated_deleted.insert(*obj_id);
                } else {
                    match by_value_objects.remove(obj_id) {
                        Some((owner, version, child_count_opt)) => {
                            state_view.log_event(Event::delete_object(
                                module_id.address(),
                                module_id.name(),
                                ctx.sender(),
                                *obj_id,
                            ));
                            if let Owner::ObjectOwner(parent_id) = owner {
                                let delta = child_count_deltas.entry(parent_id.into()).or_insert(0);
                                *delta -= 1
                            }
                            // update the child_count for the object being deleted
                            // this must be zero at the end
                            if let Some(child_count) = child_count_opt {
                                let delta = child_count_deltas.entry(*obj_id).or_insert(0);
                                *delta += child_count as i64;
                            }
                            changes
                                .insert(*obj_id, ObjectChange::Delete(version, DeleteKind::Normal));
                        }
                        None => {
                            // This object wasn't in the input, and is being deleted. It must
                            // be unwrapped in this transaction and then get deleted.
                            // When an object was wrapped at version `v`, we added an record into `parent_sync`
                            // with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
                            // we force it to `v+2` by fetching the old version from `parent_sync`.
                            // This ensures that the object_id and version pair will be unique.
                            // Here it is set to `v+1` and will be incremented in `delete_object`
                            state_view.log_event(Event::delete_object(
                                module_id.address(),
                                module_id.name(),
                                ctx.sender(),
                                *obj_id,
                            ));
                            match state_view.get_latest_parent_entry_ref(*obj_id) {
                                Ok(Some((_, previous_version, _))) => {
                                    changes.insert(
                                        *obj_id,
                                        ObjectChange::Delete(
                                            previous_version,
                                            DeleteKind::UnwrapThenDelete,
                                        ),
                                    );
                                }
                                // if the object is not in parent sync, it was wrapped before
                                // ever being stored into storage. Thus we don't need to mark it
                                // as being deleted
                                Ok(None) => (),
                                // TODO this error is (hopefully) transient and should not be
                                // a normal execution error
                                Err(_) => {
                                    return Err(ExecutionError::new_with_source(
                                        ExecutionErrorKind::InvariantViolation,
                                        missing_unwrapped_msg(obj_id),
                                    ));
                                }
                            };
                        }
                    }
                }
            }
            EventType::User => {
                match type_ {
                    TypeTag::Struct(s) => state_view.log_event(Event::move_event(
                        module_id.address(),
                        module_id.name(),
                        ctx.sender(),
                        s,
                        event_bytes,
                    )),
                    _ => unreachable!(
                        "Native function emit_event<T> ensures that T is always bound to structs"
                    ),
                };
            }
        };
    }

    // any object left in `by_value_objects` is an input passed by value that was not transferred,
    // frozen, shared, or deleted.
    // This means that either the object was wrapped inside another object that is in the Sui object
    // pool
    for (id, (owner, version, child_count_opt)) in by_value_objects {
        if let Owner::ObjectOwner(parent) = owner {
            let delta = child_count_deltas.entry(parent.into()).or_insert(0);
            *delta -= 1
        }
        if let Some(child_count) = child_count_opt {
            let delta = child_count_deltas.entry(id).or_insert(0);
            *delta += child_count as i64;
        }
        changes.insert(id, ObjectChange::Delete(version, DeleteKind::Wrap));
    }

    // Check validity of child object counts
    // - Any newly generated, and then deleted object, cannot have child objects
    // - Any deleted object (wrapped or deleted) cannot have child objects
    // - Any frozen object (made immutable) cannot have child objects
    for id in newly_generated_deleted {
        // check that the newly generated (but not used) object does not have children
        // we remove it here as it is not needed for updating the written object child counts
        let delta = child_count_deltas.remove(&id).unwrap_or(0);
        if delta != 0 {
            return Err(ExecutionErrorKind::InvalidParentDeletion {
                parent: id,
                kind: None,
            }
            .into());
        }
    }

    for id in newly_generated_unused {
        // check that any remaining non-zero delta was created this transaction and wrapped
        // we remove it here as it is not needed for updating the written object child counts
        let delta = child_count_deltas.remove(&id).unwrap_or(0);
        if delta != 0 {
            debug_assert!(delta > 0);
            return Err(ExecutionErrorKind::InvalidParentDeletion {
                parent: id,
                kind: Some(DeleteKind::Wrap),
            }
            .into());
        }
    }

    // check that all deleted objects have a child count of zero
    for (id, delta) in child_count_deltas {
        if delta == 0 {
            continue;
        }
        let change = changes.entry(id).or_insert_with(|| {
            let mut object = state_view.read_object(&id).unwrap().clone();
            // Active input object must be Move object.
            object.data.try_as_move_mut().unwrap().increment_version();
            ObjectChange::Write(object)
        });
        match change {
            ObjectChange::Write(object) => {
                object
                    .data
                    .try_as_move_mut()
                    .expect("must be move object")
                    .change_child_count(delta)
                    .map_err(|()| ExecutionErrorKind::TooManyChildObjects { object: id })?;
            }
            ObjectChange::Delete(_, kind) => {
                if delta != 0 {
                    return Err(ExecutionErrorKind::InvalidParentDeletion {
                        parent: id,
                        kind: Some(*kind),
                    }
                    .into());
                }
            }
        }
    }

    // apply object writes and object deletions
    state_view.set_create_object_ids(newly_generated_ids);
    state_view.apply_object_changes(changes);

    // check all frozen objects have a child count of zero
    for frozen_object_id in frozen_object_ids {
        let frozen_object = state_view
            .read_object(&frozen_object_id)
            .unwrap()
            .data
            .try_as_move()
            .unwrap();
        if frozen_object.child_count().unwrap_or(0) != 0 {
            return Err(ExecutionErrorKind::InvalidParentFreezing {
                parent: frozen_object_id,
            }
            .into());
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_transfer<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ParentSync,
>(
    sender: SuiAddress,
    recipient: Owner,
    type_: TypeTag,
    abilities: AbilitySet,
    contents: Vec<u8>,
    tx_digest: TransactionDigest,
    by_value_objects: &mut BTreeMap<ObjectID, (object::Owner, SequenceNumber, Option<u32>)>,
    state_view: &mut S,
    module_id: &ModuleId,
    object_owner_map: &mut BTreeMap<SuiAddress, SuiAddress>,
    newly_generated_ids: &mut BTreeSet<ObjectID>,
    newly_generated_unused: &mut BTreeSet<ObjectID>,
    frozen_object_ids: &mut BTreeSet<ObjectID>,
    child_count_deltas: &mut BTreeMap<ObjectID, i64>,
) -> Result<Object, ExecutionError> {
    let s_type = match type_ {
        TypeTag::Struct(s_type) => s_type,
        _ => unreachable!("Only structs can be transferred"),
    };
    let has_public_transfer = abilities.has_store();
    debug_assert!(abilities.has_key(), "objects should have key");
    let id = ObjectID::try_from(&contents[0..ID_END_INDEX])
        .expect("object contents should start with an id");
    newly_generated_unused.remove(&id);
    let old_object = by_value_objects.remove(&id);
    let mut is_unwrapped = !(newly_generated_ids.contains(&id) || id == SUI_SYSTEM_STATE_OBJECT_ID);
    let (version, child_count) = match old_object {
        Some((_, version, child_count)) => (version, child_count),
        // When an object was wrapped at version `v`, we added an record into `parent_sync`
        // with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
        // it will also have version `v+1`, leading to a violation of the invariant that any
        // object_id and version pair must be unique. We use the version from parent_sync and
        // increment it (below), so we will have `(v+1)+1`, thus preserving the uniqueness
        None if is_unwrapped => match state_view.get_latest_parent_entry_ref(id) {
            Ok(Some((_, last_version, _))) => (last_version, None),
            // if the object is not in parent sync, it was wrapped before ever being stored into
            // storage.
            // we set is_unwrapped to false since the object has never been in storage
            // and essentially is being created. Similarly, we add it to the newly_generated_ids set
            Ok(None) => {
                is_unwrapped = false;
                newly_generated_ids.insert(id);
                (SequenceNumber::new(), None)
            }
            Err(_) => {
                // TODO this error is (hopefully) transient and should not be
                // a normal execution error
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::InvariantViolation,
                    missing_unwrapped_msg(&id),
                ));
            }
        },
        None => (SequenceNumber::new(), None),
    };

    // safe because `has_public_transfer` was properly determined from the abilities
    let mut move_obj = unsafe {
        MoveObject::new_from_execution(s_type, has_public_transfer, version, child_count, contents)
    };

    debug_assert_eq!(id, move_obj.id());

    #[cfg(debug_assertions)]
    {
        check_transferred_object_invariants(&move_obj, &old_object)
    }

    // increment the object version. note that if the transferred object was
    // freshly created, this means that its version will now be 1.
    // thus, all objects in the global object pool have version > 0
    move_obj.increment_version();
    let obj_id = move_obj.id();
    // A to-be-transferred object can come from 3 sources:
    //   1. Passed in by-value (in `by_value_objects`, i.e. old_object is not none)
    //   2. Created in this transaction (in `newly_generated_ids`)
    //   3. Unwrapped in this transaction
    // The following condition checks if this object was unwrapped in this transaction.
    if let Some((old_owner, old_obj_ver, _)) = old_object {
        // decrement old owner count
        if let Owner::ObjectOwner(old_parent) = old_owner {
            let delta = child_count_deltas.entry(old_parent.into()).or_insert(0);
            *delta -= 1
        }
        // Some kind of transfer since there's an old object
        // Add an event for the transfer
        let transfer_type = match recipient {
            Owner::AddressOwner(_) => Some(TransferType::ToAddress),
            Owner::ObjectOwner(_) => Some(TransferType::ToObject),
            _ => None,
        };
        if let Some(type_) = transfer_type {
            state_view.log_event(Event::TransferObject {
                package_id: ObjectID::from(*module_id.address()),
                transaction_module: Identifier::from(module_id.name()),
                sender,
                recipient,
                object_id: obj_id,
                version: old_obj_ver,
                type_,
            })
        }
    } else {
        // Newly created object
        if !is_unwrapped {
            state_view.log_event(Event::new_object(
                module_id.address(),
                module_id.name(),
                sender,
                recipient,
                obj_id,
            ));
        }
    }
    let obj = Object::new_move(move_obj, recipient, tx_digest);
    if old_object.is_none() {
        // Charge extra gas based on object size if we are creating a new object.
        // TODO: Do we charge extra gas when creating new objects (on top of storage write cost)?
    }
    let obj_address: SuiAddress = obj_id.into();
    object_owner_map.remove(&obj_address);
    // increment new owner count, if applicable
    match recipient {
        Owner::Shared | Owner::AddressOwner(_) => (),
        Owner::Immutable => {
            frozen_object_ids.insert(id);
        }
        Owner::ObjectOwner(new_owner) => {
            let delta = child_count_deltas.entry(new_owner.into()).or_insert(0);
            *delta += 1;
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
                return Err(ExecutionErrorKind::circular_object_ownership(parent.into()).into());
            }
            object_owner_map.insert(obj_address, new_owner);
        }
    }

    Ok(obj)
}

#[cfg(debug_assertions)]
fn check_transferred_object_invariants(
    new_object: &MoveObject,
    old_object: &Option<(object::Owner, SequenceNumber, Option<u32>)>,
) {
    if let Some((_owner, old_version, old_child_count)) = old_object {
        // check consistency between the transferred object `new_object` and the tx input `o`
        // specifically, the object id, type, and version should be unchanged
        // we can only check the version here
        debug_assert_eq!(*old_version, new_object.version());
        debug_assert_eq!(*old_child_count, new_object.child_count());
    }
}

pub struct TypeCheckSuccess {
    pub module_id: ModuleId,
    pub object_data: BTreeMap<ObjectID, (object::Owner, SequenceNumber, Option<u32>)>,
    pub by_value_objects: BTreeSet<ObjectID>,
    pub mutable_ref_objects: BTreeMap<LocalIndex, ObjectID>,
    pub args: Vec<Vec<u8>>,
    /// is TxContext included in the arguments?
    pub has_ctx_arg: bool,
}

/// - Check that `package_object`, `module` and `function` are valid
/// - Check that the the signature of `function` is well-typed w.r.t `type_args`, `object_args`, and `pure_args`
/// - Return the ID of the resolved module, a vector of BCS encoded arguments to pass to the VM, and a partitioning
/// of the input objects into objects passed by value vs by mutable reference
pub fn resolve_and_type_check(
    objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    module: &CompiledModule,
    function: &Identifier,
    type_args: &[TypeTag],
    args: Vec<CallArg>,
    is_genesis: bool,
) -> Result<TypeCheckSuccess, ExecutionError> {
    // Resolve the function we are calling
    let view = &BinaryIndexedView::Module(module);
    let function_str = function.as_ident_str();
    let module_id = module.self_id();
    let fdef_opt = module.function_defs.iter().find(|fdef| {
        module.identifier_at(module.function_handle_at(fdef.function).name) == function_str
    });
    let fdef = match fdef_opt {
        Some(fdef) => fdef,
        None => {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::FunctionNotFound,
                format!(
                    "Could not resolve function '{}' in module {}",
                    function, &module_id,
                ),
            ));
        }
    };
    // Check for entry modifier, but ignore for genesis.
    // Genesis calls non-entry, private functions, and bypasses this rule. This is helpful for
    // ensuring the functions are not called again later.
    // In other words, this is an implementation detail that we are using `execute` for genesis
    // functions, and as such need to bypass this check.
    if !fdef.is_entry && !is_genesis {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::NonEntryFunctionInvoked,
            "Can only call `entry` functions",
        ));
    }
    let fhandle = module.function_handle_at(fdef.function);

    // check arity of type and value arguments
    if fhandle.type_parameters.len() != type_args.len() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::EntryTypeArityMismatch,
            format!(
                "Expected {:?} type arguments, but found {:?}",
                fhandle.type_parameters.len(),
                type_args.len()
            ),
        ));
    }

    // total number of args is (|objects| + |pure_args|) + 1 for the the `TxContext` object
    let parameters = &module.signature_at(fhandle.parameters).0;
    let has_ctx_arg = parameters
        .last()
        .map(|t| is_tx_context(view, t))
        .unwrap_or(false);
    let num_args = if has_ctx_arg {
        args.len() + 1
    } else {
        args.len()
    };
    if parameters.len() != num_args {
        let idx = std::cmp::min(parameters.len(), num_args) as LocalIndex;
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::ArityMismatch),
            format!(
                "Expected {:?} arguments calling function '{}', but found {:?}",
                parameters.len(),
                function,
                num_args
            ),
        ));
    }

    // type check object arguments passed in by value and by reference
    let mut object_data = BTreeMap::new();
    let mut mutable_ref_objects = BTreeMap::new();
    let mut by_value_objects = BTreeSet::new();

    // Track the mapping from each input object to its Move type.
    // This will be needed latter in `check_child_object_of_shared_object`.
    let mut object_type_map = BTreeMap::new();
    let bcs_args = args
        .into_iter()
        .enumerate()
        .map(|(idx, arg)| {
            let param_type = &parameters[idx];
            let idx = idx as LocalIndex;
            let (object_arg, arg_type, param_type) = match arg {
                CallArg::Pure(arg) => {
                    if !is_primitive(view, type_args, param_type) {
                        let msg = format!(
                            "Non-primitive argument at index {}. If it is an object, it must be \
                            populated by an object ID",
                            idx,
                        );
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::entry_argument_error(
                                idx,
                                EntryArgumentErrorKind::UnsupportedPureArg,
                            ),
                            msg,
                        ));
                    }
                    return Ok(arg);
                }
                CallArg::Object(ObjectArg::ImmOrOwnedObject(ref_)) => serialize_object(
                    InputObjectKind::ImmOrOwnedMoveObject(ref_),
                    idx,
                    param_type,
                    objects,
                    &mut object_data,
                    &mut mutable_ref_objects,
                    &mut by_value_objects,
                    &mut object_type_map,
                )?,
                CallArg::Object(ObjectArg::SharedObject(id)) => serialize_object(
                    InputObjectKind::SharedMoveObject(id),
                    idx,
                    param_type,
                    objects,
                    &mut object_data,
                    &mut mutable_ref_objects,
                    &mut by_value_objects,
                    &mut object_type_map,
                )?,
                CallArg::ObjVec(vec) => {
                    if vec.is_empty() {
                        // bcs representation of the empty vector
                        return Ok(vec![0]);
                    }
                    // write length of the vector as uleb128 as it is encoded in BCS and then append
                    // all (already serialized) object content data
                    let mut res = vec![];
                    leb128::write::unsigned(&mut res, vec.len() as u64).unwrap();
                    let mut element_type = None;
                    let mut inner_vec_type = None;
                    for arg in vec {
                        let object_kind = match arg {
                            ObjectArg::ImmOrOwnedObject(ref_) => {
                                InputObjectKind::ImmOrOwnedMoveObject(ref_)
                            }
                            ObjectArg::SharedObject(_) => {
                                let msg = format!(
                                    "Shared object part of the vector argument at index {}.\
                                     Only owned objects are allowed",
                                    idx,
                                );

                                return Err(ExecutionError::new_with_source(
                                    ExecutionErrorKind::entry_argument_error(
                                        idx,
                                        EntryArgumentErrorKind::UnsupportedPureArg,
                                    ),
                                    msg,
                                ));
                            }
                        };
                        let (o, arg_type, param_type) = serialize_object(
                            object_kind,
                            idx,
                            param_type,
                            objects,
                            &mut object_data,
                            &mut mutable_ref_objects,
                            &mut by_value_objects,
                            &mut object_type_map,
                        )?;
                        res.extend(o);
                        element_type = Some(arg_type);
                        inner_vec_type = Some(param_type);
                    }
                    (res, element_type.unwrap(), inner_vec_type.unwrap())
                }
            };

            type_check_struct(view, type_args, idx, arg_type, param_type)?;
            Ok(object_arg)
        })
        .collect::<Result<Vec<_>, _>>()?;

    check_shared_object_rules(
        objects,
        &by_value_objects,
        &object_type_map,
        module.self_id(),
    )?;

    Ok(TypeCheckSuccess {
        module_id,
        object_data,
        by_value_objects,
        mutable_ref_objects,
        args: bcs_args,
        has_ctx_arg,
    })
}

fn serialize_object<'a>(
    object_kind: InputObjectKind,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    objects: &'a BTreeMap<ObjectID, impl Borrow<Object>>,
    object_data: &mut BTreeMap<ObjectID, (Owner, SequenceNumber, Option<u32>)>,
    mutable_ref_objects: &mut BTreeMap<u8, ObjectID>,
    by_value_objects: &mut BTreeSet<ObjectID>,
    object_type_map: &mut BTreeMap<ObjectID, ModuleId>,
) -> Result<(Vec<u8>, &'a StructTag, &'a SignatureToken), ExecutionError> {
    let object_id = object_kind.object_id();
    let object = match objects.get(&object_id) {
        Some(object) => object.borrow(),
        None => {
            debug_assert!(
                false,
                "Object map not populated for arg {} with id {}",
                idx, object_id
            );
            return Err(ExecutionErrorKind::InvariantViolation.into());
        }
    };

    match object_kind {
        InputObjectKind::ImmOrOwnedMoveObject(_) if object.is_shared() => {
            let error = format!(
                "Argument at index {} populated with shared object id {} \
                        but an immutable or owned object was expected",
                idx, object_id
            );
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(
                    idx,
                    EntryArgumentErrorKind::ObjectKindMismatch,
                ),
                error,
            ));
        }
        InputObjectKind::SharedMoveObject(_) if !object.is_shared() => {
            let error = format!(
                "Argument at index {} populated with an immutable or owned object id {} \
                        but an shared object was expected",
                idx, object_id
            );
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(
                    idx,
                    EntryArgumentErrorKind::ObjectKindMismatch,
                ),
                error,
            ));
        }
        _ => (),
    }

    let move_object = match &object.data {
        Data::Move(m) => m,
        Data::Package(_) => {
            let for_vector = matches!(param_type, SignatureToken::Vector { .. });
            let error = format!(
                "Found module {} argument, but function expects {:?}",
                if for_vector { "element in vector" } else { "" },
                param_type
            );
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                error,
            ));
        }
    };

    // check that m.type_ matches the parameter types of the function
    let inner_param_type = inner_param_type(
        object,
        object_id,
        idx,
        param_type,
        &move_object.type_,
        mutable_ref_objects,
        by_value_objects,
    )?;

    object_type_map.insert(object_id, move_object.type_.module_id());
    object_data.insert(
        object_id,
        (object.owner, object.version(), move_object.child_count()),
    );
    Ok((
        move_object.contents().to_vec(),
        &move_object.type_,
        inner_param_type,
    ))
}

fn inner_param_type<'a>(
    object: &Object,
    object_id: ObjectID,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    arg_type: &StructTag,
    mutable_ref_objects: &mut BTreeMap<u8, ObjectID>,
    by_value_objects: &mut BTreeSet<ObjectID>,
) -> Result<&'a SignatureToken, ExecutionError> {
    match &param_type {
        SignatureToken::Reference(inner_t) => Ok(&**inner_t),
        SignatureToken::MutableReference(inner_t) => {
            if object.is_immutable() {
                let error = format!(
                    "Argument {} is expected to be mutable, immutable object found",
                    idx
                );
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx,
                        EntryArgumentErrorKind::InvalidObjectByMuteRef,
                    ),
                    error,
                ));
            }
            mutable_ref_objects.insert(idx as LocalIndex, object_id);
            Ok(&**inner_t)
        }
        SignatureToken::Vector(inner_t) => inner_param_type(
            object,
            object_id,
            idx,
            inner_t,
            arg_type,
            mutable_ref_objects,
            by_value_objects,
        ),
        t @ SignatureToken::Struct(_)
        | t @ SignatureToken::StructInstantiation(_, _)
        | t @ SignatureToken::TypeParameter(_) => {
            match &object.owner {
                Owner::AddressOwner(_) | Owner::ObjectOwner(_) => (),
                Owner::Shared | Owner::Immutable => {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::entry_argument_error(
                            idx,
                            EntryArgumentErrorKind::InvalidObjectByValue,
                        ),
                        format!(
                            "Immutable and shared objects cannot be passed by-value, \
                                    violation found in argument {}",
                            idx
                        ),
                    ));
                }
            }
            by_value_objects.insert(object_id);
            Ok(t)
        }
        t => {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                format!(
                    "Found object argument {}, but function expects {:?}",
                    arg_type, t
                ),
            ));
        }
    }
}

/// Check rules for shared object + by-value child rules and rules for by-value shared object rules:
/// - For each pair of a shared object and a descendant of it (through object ownership), if the
///   descendant is used by-value, at least one of the types of the shared object and the descendant
///   must be defined in the same module as the entry function being called
///   (somewhat similar to Rust's orphan rule where a trait impl must be defined in the same crate
///   as the type implementing the trait or the trait itself).
/// - For each shared object used by-value, the type of the shared object must be defined in the
///   same module as the entry function being called.
fn check_shared_object_rules(
    objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    by_value_objects: &BTreeSet<ObjectID>,
    object_type_map: &BTreeMap<ObjectID, ModuleId>,
    current_module: ModuleId,
) -> Result<(), ExecutionError> {
    let object_owner_map = objects
        .iter()
        .map(|(id, obj)| (*id, obj.borrow().owner))
        .collect();
    let ancestor_map = ObjectRootAncestorMap::new(&object_owner_map)?;
    let by_value_object_owned = object_owner_map
        .iter()
        .filter_map(|(id, owner)| match owner {
            Owner::ObjectOwner(owner) if by_value_objects.contains(id) => Some((*id, *owner)),
            _ => None,
        });
    for (child_id, owner) in by_value_object_owned {
        let (ancestor_id, ancestor_owner) = match ancestor_map.get_root_ancestor(&child_id) {
            Some(ancestor) => ancestor,
            None => return Err(ExecutionErrorKind::missing_object_owner(child_id, owner).into()),
        };
        if ancestor_owner.is_shared() {
            // unwrap safe because the object ID exists in object_owner_map.
            let child_module = object_type_map.get(&child_id).unwrap();
            let ancestor_module = object_type_map.get(&ancestor_id).unwrap();
            if !(child_module == &current_module || ancestor_module == &current_module) {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::invalid_shared_child_use(child_id, ancestor_id),
                    format!(
        "When a child object (either direct or indirect) of a shared object is passed by-value to \
        an entry function, either the child object's type or the shared object's type must be \
        defined in the same module as the called function. This is violated by object {child_id} \
        (defined in module '{child_module}'), whose ancestor {ancestor_id} is a shared \
        object (defined in module '{ancestor_module}'), and neither are defined in this module \
        '{current_module}'",
                    ),
                ));
            }
        }
    }

    // TODO not yet supported
    // // check shared object by value rule
    // let by_value_shared_object = object_owner_map
    //     .iter()
    //     .filter(|(id, owner)| matches!(owner, Owner::Shared) && by_value_objects.contains(id))
    //     .map(|(id, _)| *id);
    // for shared_object_id in by_value_shared_object {
    //     let shared_object_module = object_type_map.get(&shared_object_id).unwrap();
    //     if shared_object_module != &current_module {
    //         return Err(ExecutionError::new_with_source(
    //             ExecutionErrorKind::invalid_shared_by_value(shared_object_id),
    //             format!(
    //     "When a shared object is passed as an owned Move value in an entry function, either the \
    //     the shared object's type must be defined in the same module as the called function. The \
    //     shared object {shared_object_id} (defined in module '{shared_object_module}') is not \
    //     defined in this module '{current_module}'",
    //             ),
    //         ));
    //     }
    // }
    Ok(())
}

fn is_primitive(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    t: &SignatureToken,
) -> bool {
    match t {
        SignatureToken::Bool
        | SignatureToken::U8
        | SignatureToken::U64
        | SignatureToken::U128
        | SignatureToken::Address => true,

        SignatureToken::Struct(idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *idx);
            // is ID
            resolved_struct == RESOLVED_SUI_ID
        }

        SignatureToken::StructInstantiation(idx, targs) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *idx);
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && targs.len() == 1
                && is_primitive(view, function_type_arguments, &targs[0])
        }
        SignatureToken::Vector(inner) => is_primitive(view, function_type_arguments, inner),

        SignatureToken::TypeParameter(idx) => function_type_arguments
            .get(*idx as usize)
            .map(is_primitive_type_tag)
            .unwrap_or(false),

        SignatureToken::Signer
        | SignatureToken::Reference(_)
        | SignatureToken::MutableReference(_) => false,
    }
}

fn is_primitive_type_tag(t: &TypeTag) -> bool {
    match t {
        TypeTag::Bool | TypeTag::U8 | TypeTag::U64 | TypeTag::U128 | TypeTag::Address => true,
        TypeTag::Vector(inner) => is_primitive_type_tag(inner),
        TypeTag::Struct(StructTag {
            address,
            module,
            name,
            type_params: type_args,
        }) => {
            let resolved_struct = (address, module.as_ident_str(), name.as_ident_str());
            // is id or..
            if resolved_struct == RESOLVED_SUI_ID {
                return true;
            }
            // is option of a primitive
            resolved_struct == RESOLVED_STD_OPTION
                && type_args.len() == 1
                && is_primitive_type_tag(&type_args[0])
        }
        TypeTag::Signer => false,
    }
}

fn type_check_struct(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    idx: LocalIndex,
    arg_type: &StructTag,
    param_type: &SignatureToken,
) -> Result<(), ExecutionError> {
    if !struct_tag_equals_sig_token(view, function_type_arguments, arg_type, param_type) {
        Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
            format!(
                "Expected argument of type {}, but found type {}",
                sui_verifier::format_signature_token(view, param_type),
                arg_type
            ),
        ))
    } else {
        Ok(())
    }
}

fn type_tag_equals_sig_token(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &TypeTag,
    param_type: &SignatureToken,
) -> bool {
    match (arg_type, param_type) {
        (TypeTag::Bool, SignatureToken::Bool)
        | (TypeTag::U8, SignatureToken::U8)
        | (TypeTag::U64, SignatureToken::U64)
        | (TypeTag::U128, SignatureToken::U128)
        | (TypeTag::Address, SignatureToken::Address)
        | (TypeTag::Signer, SignatureToken::Signer) => true,

        (TypeTag::Vector(inner_arg_type), SignatureToken::Vector(inner_param_type)) => {
            type_tag_equals_sig_token(
                view,
                function_type_arguments,
                inner_arg_type,
                inner_param_type,
            )
        }

        (TypeTag::Struct(arg_struct), SignatureToken::Struct(_))
        | (TypeTag::Struct(arg_struct), SignatureToken::StructInstantiation(_, _)) => {
            struct_tag_equals_sig_token(view, function_type_arguments, arg_struct, param_type)
        }

        (_, SignatureToken::TypeParameter(idx)) => {
            arg_type == &function_type_arguments[*idx as usize]
        }
        _ => false,
    }
}

fn struct_tag_equals_sig_token(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &StructTag,
    param_type: &SignatureToken,
) -> bool {
    match param_type {
        SignatureToken::Struct(idx) => {
            struct_tag_equals_struct_inst(view, function_type_arguments, arg_type, *idx, &[])
        }
        SignatureToken::StructInstantiation(idx, args) => {
            struct_tag_equals_struct_inst(view, function_type_arguments, arg_type, *idx, args)
        }
        SignatureToken::TypeParameter(idx) => match &function_type_arguments[*idx as usize] {
            TypeTag::Struct(s) => arg_type == s,
            _ => false,
        },
        _ => false,
    }
}

fn struct_tag_equals_struct_inst(
    view: &BinaryIndexedView,
    function_type_arguments: &[TypeTag],
    arg_type: &StructTag,
    param_type: StructHandleIndex,
    param_type_arguments: &[SignatureToken],
) -> bool {
    let (address, module_name, struct_name) = sui_verifier::resolve_struct(view, param_type);

    // same address, module, name, and type parameters
    &arg_type.address == address
        && arg_type.module.as_ident_str() == module_name
        && arg_type.name.as_ident_str() == struct_name
        && arg_type.type_params.len() == param_type_arguments.len()
        && arg_type.type_params.iter().zip(param_type_arguments).all(
            |(arg_type_arg, param_type_arg)| {
                type_tag_equals_sig_token(
                    view,
                    function_type_arguments,
                    arg_type_arg,
                    param_type_arg,
                )
            },
        )
}

fn missing_unwrapped_msg(id: &ObjectID) -> String {
    format!(
        "Unable to unwrap object {}. Was unable to retrieve last known version in the parent sync",
        id
    )
}
