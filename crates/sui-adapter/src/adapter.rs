// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet, HashSet},
    convert::TryFrom,
    fmt::Debug,
};

use anyhow::Result;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    errors::PartialVMResult,
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
    id::VersionedID,
    messages::{CallArg, InputObjectKind, ObjectArg},
    object::{self, Data, MoveObject, Object, Owner},
    storage::{DeleteKind, Storage},
    SUI_SYSTEM_STATE_OBJECT_ID,
};
use sui_verifier::{
    entry_points_verifier::{is_tx_context, INIT_FN_NAME, RESOLVED_STD_OPTION, RESOLVED_SUI_ID},
    verifier,
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
pub fn execute<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
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
                Some((*id, state_view.read_object(id)?))
            }
        })
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
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    vm: &MoveVM,
    state_view: &mut S,
    module_id: &ModuleId,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    args: Vec<Vec<u8>>,
    has_ctx_arg: bool,
    object_data: BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
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
        .filter_map(|(id, (owner, _))| match owner {
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

pub fn publish<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    module_bytes: Vec<Vec<u8>>,
    ctx: &mut TxContext,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    gas_status.charge_publish_package(module_bytes.iter().map(|v| v.len()).sum())?;
    let mut modules = module_bytes
        .iter()
        .map(|b| CompiledModule::deserialize(b))
        .collect::<PartialVMResult<Vec<CompiledModule>>>()?;

    if modules.is_empty() {
        return Err(ExecutionErrorKind::ModulePublishFailure.into());
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
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
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
            module.function_defs.iter().find(|fdef| {
                let fhandle = module.function_handle_at(fdef.function).name;
                let fname = module.identifier_at(fhandle);
                fname == INIT_FN_NAME
            })?;
            Some(module.self_id())
        })
        .collect();

    // wrap the modules in an object, write it to the store
    // The call to unwrap() will go away once we remove address owner from Immutable objects.
    let package_object = Object::new_package(modules, ctx.digest());
    state_view.set_create_object_ids(HashSet::from([package_object.id()]));
    state_view.write_object(package_object);

    init_modules(state_view, vm, modules_to_init, ctx, gas_status)
}

/// Modules in module_ids_to_init must have the init method defined
fn init_modules<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    vm: &MoveVM,
    module_ids_to_init: Vec<ModuleId>,
    ctx: &mut TxContext,
    gas_status: &mut SuiGasStatus,
) -> Result<(), ExecutionError> {
    let init_ident = Identifier::new(INIT_FN_NAME.as_str()).unwrap();
    for module_id in module_ids_to_init {
        let args = vec![ctx.to_vec()];
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
                ExecutionErrorKind::ModulePublishFailure,
                format!("Publishing module {name} with non-zero address is not allowed"),
            ));
        }
        let new_module_id = ModuleId::new(
            AccountAddress::from(package_id),
            old_module_id.name().to_owned(),
        );
        if sub_map.insert(old_module_id, new_module_id).is_some() {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::ModulePublishFailure,
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
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    state_view: &mut S,
    module_id: &ModuleId,
    mut by_value_objects: BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
    mutable_refs: Vec<(ObjectID, Vec<u8>)>,
    events: Vec<MoveEvent>,
    ctx: &TxContext,
    mut object_owner_map: BTreeMap<SuiAddress, SuiAddress>,
) -> Result<(), ExecutionError> {
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
        state_view.write_object(obj);
    }
    let tx_digest = ctx.digest();
    // newly_generated_ids contains all object IDs generated in this transaction.
    // TODO: In the case of the special system transaction that creates the system state object,
    // there is one extra object created with ID hardcoded at 0x5, and it won't be included in
    // `newly_generated_ids`. To cope with this, we special check the ID inside `handle_transfer`.
    // It's a bit hacky. Ideally, we want `newly_generated_ids` to include it. But it's unclear how.
    let newly_generated_ids = ctx.recreate_all_ids();
    state_view.set_create_object_ids(newly_generated_ids.clone());
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
                handle_transfer(
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
                    &newly_generated_ids,
                )
            }
            EventType::DeleteObjectID => {
                // unwrap safe because this event can only be emitted from processing
                // native call delete_id, which guarantees the type of the id.
                let id: VersionedID = bcs::from_bytes(&event_bytes).unwrap();
                let obj_id = id.object_id();
                // We don't care about IDs that are generated in this same transaction
                // but only to be deleted.
                if !newly_generated_ids.contains(obj_id) {
                    match by_value_objects.remove(id.object_id()) {
                        Some((Owner::ObjectOwner { .. }, _)) => {
                            // If an object is owned by another object, we are not allowed to directly delete the child
                            // object because this could lead to a dangling reference of the ownership. Such
                            // dangling reference can never be dropped. To delete this object, one must either first transfer
                            // the child object to an account address, or call through transfer::delete_child_object(),
                            // which would consume both the child object and the ChildRef ownership reference,
                            // and emit the DeleteChildObject event. These child objects can be safely deleted.
                            return Err(ExecutionErrorKind::DeleteObjectOwnedObject.into());
                        }
                        Some(_) => {
                            state_view.log_event(Event::delete_object(
                                module_id.address(),
                                module_id.name(),
                                ctx.sender(),
                                *obj_id,
                            ));
                            state_view.delete_object(obj_id, id.version(), DeleteKind::Normal)
                        }
                        None => {
                            // This object wasn't in the input, and is being deleted. It must
                            // be unwrapped in this transaction and then get deleted.
                            // When an object was wrapped at version `v`, we added an record into `parent_sync`
                            // with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
                            // it will also have version `v+1`, leading to a violation of the invariant that any
                            // object_id and version pair must be unique. Hence for any object that's just unwrapped,
                            // we force incrementing its version number again to make it `v+2` before writing to the store.
                            state_view.log_event(Event::delete_object(
                                module_id.address(),
                                module_id.name(),
                                ctx.sender(),
                                *obj_id,
                            ));
                            state_view.delete_object(
                                obj_id,
                                id.version().increment(),
                                DeleteKind::UnwrapThenDelete,
                            )
                        }
                    }
                }
                Ok(())
            }
            EventType::DeleteChildObject => {
                let id_bytes: AccountAddress = bcs::from_bytes(&event_bytes).unwrap();
                let obj_id: ObjectID = id_bytes.into();
                // unwrap safe since to delete a child object, this child object
                // must be passed by value in the input.
                let (_owner, version) = by_value_objects.remove(&obj_id).unwrap();
                state_view.delete_object(&obj_id, version, DeleteKind::Normal);
                Ok(())
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
                Ok(())
            }
        }?;
    }

    // any object left in `by_value_objects` is an input passed by value that was not transferred or frozen.
    // this means that either the object was (1) deleted from the Sui system altogether, or
    // (2) wrapped inside another object that is in the Sui object pool
    for (id, (_owner, version)) in by_value_objects {
        state_view.delete_object(&id, version, DeleteKind::Wrap);
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn handle_transfer<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    sender: SuiAddress,
    recipient: Owner,
    type_: TypeTag,
    abilities: AbilitySet,
    contents: Vec<u8>,
    tx_digest: TransactionDigest,
    by_value_objects: &mut BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
    state_view: &mut S,
    module_id: &ModuleId,
    object_owner_map: &mut BTreeMap<SuiAddress, SuiAddress>,
    newly_generated_ids: &HashSet<ObjectID>,
) -> Result<(), ExecutionError> {
    match type_ {
        TypeTag::Struct(s_type) => {
            let has_public_transfer = abilities.has_store();
            // safe because `has_public_transfer` was properly determined from the abilities
            let mut move_obj =
                unsafe { MoveObject::new_from_execution(s_type, has_public_transfer, contents) };
            let old_object = by_value_objects.remove(&move_obj.id());

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
            if old_object.is_none() {
                // Newly created object
                if newly_generated_ids.contains(&obj_id) || obj_id == SUI_SYSTEM_STATE_OBJECT_ID {
                    state_view.log_event(Event::new_object(
                        module_id.address(),
                        module_id.name(),
                        sender,
                        recipient,
                        obj_id,
                    ));
                } else {
                    // When an object was wrapped at version `v`, we added an record into `parent_sync`
                    // with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
                    // it will also have version `v+1`, leading to a violation of the invariant that any
                    // object_id and version pair must be unique. Hence for any object that's just unwrapped,
                    // we force incrementing its version number again to make it `v+2` before writing to the store.
                    move_obj.increment_version();
                }
            } else if let Some((_, old_obj_ver)) = old_object {
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
            }
            let obj = Object::new_move(move_obj, recipient, tx_digest);
            if old_object.is_none() {
                // Charge extra gas based on object size if we are creating a new object.
                // TODO: Do we charge extra gas when creating new objects (on top of storage write cost)?
            }
            let obj_address: SuiAddress = obj_id.into();
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
                    return Err(ExecutionErrorKind::CircularObjectOwnership.into());
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
fn check_transferred_object_invariants(
    new_object: &MoveObject,
    old_object: &Option<(object::Owner, SequenceNumber)>,
) {
    if let Some((_owner, old_version)) = old_object {
        // check consistency between the transferred object `new_object` and the tx input `o`
        // specifically, the object id, type, and version should be unchanged
        // we can only check the version here
        debug_assert_eq!(*old_version, new_object.version());
    }
}

pub struct TypeCheckSuccess {
    pub module_id: ModuleId,
    pub object_data: BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
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
            ExecutionErrorKind::InvalidNonEntryFunction,
            "Can only call `entry` functions",
        ));
    }
    let fhandle = module.function_handle_at(fdef.function);

    // check arity of type and value arguments
    if fhandle.type_parameters.len() != type_args.len() {
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::InvalidFunctionSignature,
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
        return Err(ExecutionError::new_with_source(
            ExecutionErrorKind::InvalidFunctionSignature,
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
            let object_kind = match arg {
                CallArg::Pure(arg) => {
                    if !is_primitive(view, type_args, param_type) {
                        return Err(
                            ExecutionError::new_with_source(
                                ExecutionErrorKind::TypeError,
                                format!(
                                    "Non-primitive argument at index {}. If it is an object, it must be populated by an object ID",
                                    idx,
                                ),
                            )
                        );
                    }
                    return Ok(arg);
                }
                CallArg::Object(ObjectArg::ImmOrOwnedObject(ref_)) => {
                    InputObjectKind::ImmOrOwnedMoveObject(ref_)
                }
                CallArg::Object(ObjectArg::SharedObject(id)) => {
                    InputObjectKind::SharedMoveObject(id)
                }
            };

            let id = object_kind.object_id();
            let object = match objects.get(&id) {
                Some(object) => object.borrow(),
                None => {
                    debug_assert!(
                        false,
                        "Object map not populated for arg {} with id {}",
                        idx, id
                    );
                    return Err(ExecutionErrorKind::ExecutionInvariantViolation.into());
                }
            };
            match object_kind {
                InputObjectKind::ImmOrOwnedMoveObject(_) if object.is_shared() => {
                    let error = format!(
                        "Argument at index {} populated with shared object id {} \
                        but an immutable or owned object was expected",
                        idx, id
                    );
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::TypeError,
                        error,
                    ));
                }
                InputObjectKind::SharedMoveObject(_) if !object.is_shared() => {
                    let error = format!(
                        "Argument at index {} populated with an immutable or owned object id {} \
                        but an shared object was expected",
                        idx, id
                    );
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::TypeError,
                        error,
                    ));
                }
                _ => (),
            }

            object_data.insert(id, (object.owner, object.version()));
            let move_object = match &object.data {
                Data::Move(m) => m,
                Data::Package(_) => {
                    let error = format!(
                        "Found module argument, but function expects {:?}",
                        param_type
                    );
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::TypeError,
                        error,
                    ));
                }
            };
            let object_arg = move_object.contents().to_vec();
            // check that m.type_ matches the parameter types of the function
            let inner_param_type = match &param_type {
                SignatureToken::Reference(inner_t) => &**inner_t,
                SignatureToken::MutableReference(inner_t) => {
                    if object.is_immutable() {
                        let error = format!(
                            "Argument {} is expected to be mutable, immutable object found",
                            idx
                        );
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::TypeError,
                            error,
                        ));
                    }
                    mutable_ref_objects.insert(idx as LocalIndex, id);
                    &**inner_t
                }
                t @ SignatureToken::Struct(_)
                | t @ SignatureToken::StructInstantiation(_, _)
                | t @ SignatureToken::TypeParameter(_) => {
                    if !object.is_owned() {
                        // Forbid passing shared (both mutable and immutable) object by value.
                        // This ensures that shared object cannot be transferred, deleted or wrapped.
                        return Err(ExecutionError::new_with_source(
                            ExecutionErrorKind::TypeError,
                            format!(
                                "Only owned object can be passed by-value, violation found in argument {}",
                                idx
                            ),
                        ));
                    }
                    by_value_objects.insert(id);
                    t
                }
                t => {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::TypeError,
                        format!(
                            "Found object argument {}, but function expects {:?}",
                            move_object.type_, t
                        ),
                    ));
                }
            };
            type_check_struct(view, type_args, &move_object.type_, inner_param_type)?;
            object_type_map.insert(id, move_object.type_.module_id());
            Ok(object_arg)
        })
        .collect::<Result<Vec<_>, _>>()?;

    check_child_object_of_shared_object(objects, &object_type_map, module.self_id())?;

    Ok(TypeCheckSuccess {
        module_id,
        object_data,
        by_value_objects,
        mutable_ref_objects,
        args: bcs_args,
        has_ctx_arg,
    })
}

/// Check that for each pair of a shared object and a descendant of it (through object ownership),
/// at least one of the types of the shared object and the descendant must be defined in the
/// same module as the function being called (somewhat similar to Rust's orphan rule).
fn check_child_object_of_shared_object(
    objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    object_type_map: &BTreeMap<ObjectID, ModuleId>,
    current_module: ModuleId,
) -> Result<(), ExecutionError> {
    let object_owner_map = objects
        .iter()
        .map(|(id, obj)| (*id, obj.borrow().owner))
        .collect();
    let ancestor_map = ObjectRootAncestorMap::new(&object_owner_map)?;
    for (object_id, owner) in object_owner_map {
        // We are only interested in objects owned by objects.
        if !matches!(owner, Owner::ObjectOwner(..)) {
            continue;
        }
        let (ancestor_id, ancestor_owner) = ancestor_map.get_root_ancestor(&object_id)?;
        if ancestor_owner.is_shared() {
            // unwrap safe because the object ID exists in object_owner_map.
            let child_module = object_type_map.get(&object_id).unwrap();
            let ancestor_module = object_type_map.get(&ancestor_id).unwrap();
            if !(child_module == &current_module || ancestor_module == &current_module) {
                return Err(ExecutionError::new_with_source(ExecutionErrorKind::InvalidSharedChildUse,
                    format!(
        "When an (either direct or indirect) child object of a shared object is passed as a Move argument,\
        either the child object's type or the shared object's type must be defined in the same module \
        as the called function. This is violated by object {child} (defined in module '{child_module}'), \
        whose ancestor {ancestor} is a shared object (defined in module '{ancestor_module}'), \
        and neither are defined in this module '{current_module}'",
                    child = object_id,
                    child_module = current_module,
                    ancestor = ancestor_id,
                    )));
            }
        }
    }
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
    arg_type: &StructTag,
    param_type: &SignatureToken,
) -> Result<(), ExecutionError> {
    if !struct_tag_equals_sig_token(view, function_type_arguments, arg_type, param_type) {
        Err(ExecutionError::new_with_source(
            ExecutionErrorKind::TypeError,
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
