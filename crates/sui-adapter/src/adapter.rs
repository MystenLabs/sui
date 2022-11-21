// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::{
    borrow::Borrow,
    collections::{BTreeMap, BTreeSet},
    fmt::Debug,
};

use anyhow::Result;
use leb128;
use linked_hash_map::LinkedHashMap;
use move_binary_format::{
    access::ModuleAccess,
    binary_views::BinaryIndexedView,
    errors::VMResult,
    file_format::{AbilitySet, CompiledModule, LocalIndex, SignatureToken, StructHandleIndex},
};
use move_bytecode_verifier::VerifierConfig;
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    resolver::{ModuleResolver, ResourceResolver},
    value::{MoveStruct, MoveTypeLayout, MoveValue},
};
pub use move_vm_runtime::move_vm::MoveVM;
use move_vm_runtime::{
    native_extensions::NativeContextExtensions,
    native_functions::NativeFunctionTable,
    session::{SerializedReturnValues, Session},
};

use sui_cost_tables::bytecode_tables::GasStatus;
use sui_framework::natives::object_runtime::{self, ObjectRuntime};
use sui_json::primitive_type;
use sui_types::storage::SingleTxContext;
use sui_types::{
    base_types::*,
    error::ExecutionError,
    error::{ExecutionErrorKind, SuiError},
    event::Event,
    messages::{CallArg, EntryArgumentErrorKind, InputObjectKind, ObjectArg},
    object::{self, Data, MoveObject, Object, Owner, ID_END_INDEX},
    storage::{ChildObjectResolver, DeleteKind, ObjectChange, ParentSync, Storage, WriteKind},
};
use sui_verifier::{
    entry_points_verifier::{is_tx_context, RESOLVED_ASCII_STR, RESOLVED_UTF8_STR},
    verifier, INIT_FN_NAME,
};
use tracing::instrument;

use crate::bytecode_rewriter::ModuleHandleRewriter;

macro_rules! assert_invariant {
    ($cond:expr, $msg:expr) => {
        if !$cond {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::InvariantViolation,
                $msg,
            ));
        }
    };
}

pub fn new_move_vm(natives: NativeFunctionTable) -> Result<MoveVM, SuiError> {
    MoveVM::new_with_verifier_config(
        natives,
        VerifierConfig {
            max_loop_depth: Some(5),
            treat_friend_as_private: true,
            max_generic_instantiation_length: Some(32),
            max_function_parameters: Some(128),
            max_basic_blocks: Some(1024),
        },
    )
    .map_err(|_| SuiError::ExecutionInvariantViolation)
}

pub fn new_session<
    'v,
    'r,
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + ChildObjectResolver,
>(
    vm: &'v MoveVM,
    state_view: &'r S,
    input_objects: BTreeMap<ObjectID, (/* by_value */ bool, Owner)>,
) -> Session<'r, 'v, S> {
    let mut extensions = NativeContextExtensions::default();
    extensions.add(ObjectRuntime::new(Box::new(state_view), input_objects));
    vm.new_session_with_extensions(state_view, extensions)
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
#[instrument(name = "adapter_execute", level = "trace", skip_all)]
pub fn execute<
    E: Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    vm: &MoveVM,
    state_view: &mut S,
    module_id: ModuleId,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    args: Vec<CallArg>,
    gas_status: &mut GasStatus,
    ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    let objects = args
        .iter()
        .filter_map(|arg| match arg {
            CallArg::Pure(_) => None,
            CallArg::Object(ObjectArg::ImmOrOwnedObject((id, _, _)))
            | CallArg::Object(ObjectArg::SharedObject { id, .. }) => {
                Some(vec![(*id, state_view.read_object(id)?)])
            }
            CallArg::ObjVec(vec) => {
                if vec.is_empty() {
                    return None;
                }
                Some(
                    vec.iter()
                        .filter_map(|obj_arg| match obj_arg {
                            ObjectArg::ImmOrOwnedObject((id, _, _))
                            | ObjectArg::SharedObject { id, .. } => {
                                Some((*id, state_view.read_object(id)?))
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
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
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
    gas_status: &mut GasStatus, // gas status for the current call operation
    ctx: &mut TxContext,
) -> Result<(), ExecutionError> {
    let input_objects = object_data
        .iter()
        .map(|(id, (owner, _))| (*id, (by_value_objects.contains(id), *owner)))
        .collect();
    let mut session = new_session(vm, state_view, input_objects);
    // script visibility checked manually for entry points
    let (
        SerializedReturnValues {
            mut mutable_reference_outputs,
            return_values,
        },
        (change_set, events, mut native_context_extensions),
    ) = session
        .execute_function_bypass_visibility(module_id, function, type_args, args, gas_status)
        .and_then(|ret| Ok((ret, session.finish_with_extensions()?)))?;
    assert_invariant!(return_values.is_empty(), "Return values must be empty");
    let object_runtime: ObjectRuntime = native_context_extensions.remove();
    std::mem::drop(native_context_extensions);

    // Sui Move programs should never touch global state, so ChangeSet should be empty
    assert_invariant!(change_set.accounts().is_empty(), "Change set must be empty");
    // Sui Move no longer uses Move's internal event system
    assert_invariant!(events.is_empty(), "Events must be empty");
    // Input ref parameters we put in should be the same number we get out, plus one for the &mut TxContext
    let num_mut_objects = if has_ctx_arg {
        mutable_ref_objects.len() + 1
    } else {
        mutable_ref_objects.len()
    };
    assert_invariant!(
        num_mut_objects == mutable_reference_outputs.len(),
        "Number of mutable references returned does not match input"
    );

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
    assert_invariant!(
        mutable_ref_objects.is_empty(),
        "All mutable references should have been marked as updated"
    );
    let by_value_object_map = object_data
        .into_iter()
        .filter(|(id, _obj)| by_value_objects.contains(id))
        .collect();
    let object_runtime::RuntimeResults {
        writes,
        deletions,
        user_events,
        loaded_child_objects,
    } = object_runtime.finish()?;
    let session = new_session(vm, &*state_view, BTreeMap::new());
    let writes = writes
        .into_iter()
        .map(|(id, (write_kind, owner, ty, tag, value))| {
            let abilities = session.get_type_abilities(&ty)?;
            let layout = session.get_type_layout(&TypeTag::Struct(tag.clone()))?;
            let bytes = value.simple_serialize(&layout).unwrap();
            Ok((id, (write_kind, owner, tag, abilities, bytes)))
        })
        .collect::<VMResult<_>>()?;
    let user_events = user_events
        .into_iter()
        .map(|(_ty, tag, value)| {
            let layout = session.get_type_layout(&TypeTag::Struct(tag.clone()))?;
            let bytes = value.simple_serialize(&layout).unwrap();
            Ok((tag, bytes))
        })
        .collect::<VMResult<_>>()?;
    let (empty_changes, empty_events) = session.finish()?;
    debug_assert!(empty_changes.into_inner().is_empty());
    debug_assert!(empty_events.is_empty());
    process_successful_execution(
        state_view,
        module_id,
        &by_value_object_map,
        &loaded_child_objects,
        mutable_refs,
        writes,
        deletions,
        user_events,
        ctx,
    )?;

    Ok(())
}

#[instrument(name = "adapter_publish", level = "trace", skip_all)]
pub fn publish<
    E: Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    module_bytes: Vec<Vec<u8>>,
    ctx: &mut TxContext,
    gas_status: &mut GasStatus,
) -> Result<(), ExecutionError> {
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
    store_package_and_init_modules(state_view, &vm, modules, ctx, gas_status)
}

/// Store package in state_view and call module initializers
pub fn store_package_and_init_modules<
    E: Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    state_view: &mut S,
    vm: &MoveVM,
    modules: Vec<CompiledModule>,
    ctx: &mut TxContext,
    gas_status: &mut GasStatus,
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
    let changes = BTreeMap::from([(
        id,
        ObjectChange::Write(
            SingleTxContext::publish(ctx.sender()),
            package_object,
            WriteKind::Create,
        ),
    )]);
    state_view.apply_object_changes(changes);

    init_modules(state_view, vm, modules_to_init, ctx, gas_status)
}

/// Modules in module_ids_to_init must have the init method defined
fn init_modules<
    E: Debug,
    S: ResourceResolver<Error = E>
        + ModuleResolver<Error = E>
        + Storage
        + ParentSync
        + ChildObjectResolver,
>(
    state_view: &mut S,
    vm: &MoveVM,
    module_ids_to_init: Vec<(ModuleId, usize)>,
    ctx: &mut TxContext,
    gas_status: &mut GasStatus,
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
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage + ChildObjectResolver,
>(
    state_view: &S,
    modules: &[CompiledModule],
    package_id: ObjectID,
    natives: NativeFunctionTable,
    gas_status: &mut GasStatus,
) -> Result<MoveVM, ExecutionError> {
    // Run the Move bytecode verifier and linker.
    // It is important to do this before running the Sui verifier, since the sui
    // verifier may assume well-formedness conditions enforced by the Move verifier hold
    let vm = MoveVM::new(natives)
        .expect("VM creation only fails if natives are invalid, and we created the natives");
    let mut session = new_session(&vm, state_view, BTreeMap::new());
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
        gas_status,
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

/// Update `state_view` with the effects of successfully executing a transaction:
/// - Look for each input in `by_value_objects` to determine whether the object was transferred, frozen, or deleted
/// - Update objects passed via a mutable reference in `mutable_refs` to their new values
/// - Process creation of new objects and user-emittd events in `events`
#[allow(clippy::too_many_arguments)]
fn process_successful_execution<S: Storage + ParentSync>(
    state_view: &mut S,
    module_id: &ModuleId,
    by_value_objects: &BTreeMap<ObjectID, (object::Owner, SequenceNumber)>,
    loaded_child_objects: &BTreeMap<ObjectID, SequenceNumber>,
    mutable_refs: Vec<(ObjectID, Vec<u8>)>,
    writes: LinkedHashMap<ObjectID, (WriteKind, Owner, StructTag, AbilitySet, Vec<u8>)>,
    deletions: LinkedHashMap<ObjectID, DeleteKind>,
    user_events: Vec<(StructTag, Vec<u8>)>,
    ctx: &TxContext,
) -> Result<(), ExecutionError> {
    let sender = ctx.sender();
    let tx_ctx = SingleTxContext {
        package_id: ObjectID::from(*module_id.address()),
        transaction_module: Identifier::from(module_id.name()),
        sender,
    };
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

        changes.insert(
            obj_id,
            ObjectChange::Write(tx_ctx.clone(), obj, WriteKind::Mutate),
        );
    }
    let tx_digest = ctx.digest();

    for (id, (write_kind, mut recipient, tag, abilities, contents)) in writes {
        let has_public_transfer = abilities.has_store();
        debug_assert_eq!(
            id,
            ObjectID::from_bytes(contents.get(0..ID_END_INDEX).ok_or_else(|| {
                ExecutionError::new_with_source(
                    ExecutionErrorKind::InvariantViolation,
                    "Cannot parse Object ID",
                )
            })?)
            .expect("object contents should start with an id")
        );
        let old_object_opt = by_value_objects.get(&id);
        let loaded_child_version_opt = loaded_child_objects.get(&id);
        assert_invariant!(
            old_object_opt.is_none() || loaded_child_version_opt.is_none(),
            format!("Loaded {id} as a child, but that object was an input object")
        );
        let old_obj_ver = old_object_opt
            .map(|(_, version)| *version)
            .or_else(|| loaded_child_version_opt.copied());
        let (write_kind, version) = match (write_kind, old_obj_ver) {
            (_, Some(version)) => {
                debug_assert!(write_kind == WriteKind::Mutate);
                (WriteKind::Mutate, version)
            }
            // When an object was wrapped at version `v`, we added a record into `parent_sync`
            // with version `v+1` along with OBJECT_DIGEST_WRAPPED. Now when the object is unwrapped,
            // it will also have version `v+1`, leading to a violation of the invariant that any
            // object_id and version pair must be unique. We use the version from parent_sync and
            // increment it (below), so we will have `(v+1)+1`, thus preserving the uniqueness
            (WriteKind::Unwrap, None) => match state_view.get_latest_parent_entry_ref(id) {
                Ok(Some((_, last_version, _))) => (WriteKind::Unwrap, last_version),
                // if the object is not in parent sync, it was wrapped before ever being stored into
                // storage.
                // we set is_unwrapped to false since the object has never been in storage
                // and essentially is being created. Similarly, we add it to the newly_generated_ids
                // set
                Ok(None) => (WriteKind::Create, SequenceNumber::new()),
                _ => {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvariantViolation,
                        missing_unwrapped_msg(&id),
                    ));
                }
            },
            (_, None) => {
                debug_assert!(write_kind == WriteKind::Create);
                (WriteKind::Create, SequenceNumber::new())
            }
        };
        // safe because `has_public_transfer` was properly determined from the abilities
        let mut move_obj =
            unsafe { MoveObject::new_from_execution(tag, has_public_transfer, version, contents) };

        #[cfg(debug_assertions)]
        {
            check_transferred_object_invariants(&move_obj, &old_obj_ver)
        }

        // increment the object version. note that if the transferred object was
        // freshly created, this means that its version will now be 1.
        // thus, all objects in the global object pool have version > 0
        move_obj.increment_version();

        // Remember the version this object was shared at, if this write was the one that shared it.
        if let Owner::Shared {
            initial_shared_version,
        } = &mut recipient
        {
            assert_invariant!(
                old_obj_ver.is_none(),
                "The object should be guaranteed to be new by \
                sui::transfer::share_object, which aborts if it is not new"
            );
            // TODO Consider a distinct Recipient enum within ObjectRuntime to enforce this
            // invariant at the type level.
            assert_eq!(
                *initial_shared_version,
                SequenceNumber::new(),
                "Initial version should be blank before this point",
            );
            *initial_shared_version = move_obj.version();
        }

        let obj = Object::new_move(move_obj, recipient, tx_digest);
        if old_obj_ver.is_none() {
            // Charge extra gas based on object size if we are creating a new object.
            // TODO: Do we charge extra gas when creating new objects (on top of storage write cost)?
        }
        changes.insert(id, ObjectChange::Write(tx_ctx.clone(), obj, write_kind));
    }

    for (id, delete_kind) in deletions {
        let version = match by_value_objects.get(&id) {
            Some((_, version)) => *version,
            None => match state_view.get_latest_parent_entry_ref(id) {
                Ok(Some((_, previous_version, _))) => previous_version,
                Ok(None) => {
                    // TODO we don't really need to mark this as deleted, as the object was not
                    // created this txn but has never existed in storage. We just need a better
                    // way of detecting this rather than relying on the parent sync
                    SequenceNumber::new()
                }
                _ => {
                    return Err(ExecutionError::new_with_source(
                        ExecutionErrorKind::InvariantViolation,
                        missing_unwrapped_msg(&id),
                    ))
                }
            },
        };
        changes.insert(
            id,
            ObjectChange::Delete(tx_ctx.clone(), version, delete_kind),
        );
    }

    for (tag, contents) in user_events {
        state_view.log_event(Event::move_event(
            module_id.address(),
            module_id.name(),
            ctx.sender(),
            tag,
            contents,
        ))
    }

    // apply object writes and object deletions
    state_view.apply_object_changes(changes);

    Ok(())
}

#[cfg(debug_assertions)]
fn check_transferred_object_invariants(
    new_object: &MoveObject,
    old_object: &Option<SequenceNumber>,
) {
    if let Some(old_version) = old_object {
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
            let object_arg = match arg {
                CallArg::Pure(arg) => {
                    let (is_primitive, type_layout_opt) =
                        primitive_type(view, type_args, param_type);
                    if !is_primitive {
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
                    validate_primitive_arg(view, &arg, idx, param_type, type_layout_opt)?;
                    return Ok(arg);
                }
                CallArg::Object(ObjectArg::ImmOrOwnedObject(ref_)) => {
                    let (o, arg_type, param_type) = serialize_object(
                        InputObjectKind::ImmOrOwnedMoveObject(ref_),
                        idx,
                        param_type,
                        objects,
                        &mut object_data,
                        &mut mutable_ref_objects,
                        &mut by_value_objects,
                        &mut object_type_map,
                    )?;
                    type_check_struct(view, type_args, idx, arg_type, param_type)?;
                    o
                }
                CallArg::Object(ObjectArg::SharedObject {
                    id,
                    initial_shared_version,
                }) => {
                    let (o, arg_type, param_type) = serialize_object(
                        InputObjectKind::SharedMoveObject {
                            id,
                            initial_shared_version,
                        },
                        idx,
                        param_type,
                        objects,
                        &mut object_data,
                        &mut mutable_ref_objects,
                        &mut by_value_objects,
                        &mut object_type_map,
                    )?;
                    type_check_struct(view, type_args, idx, arg_type, param_type)?;
                    o
                }
                CallArg::ObjVec(vec) => {
                    if vec.is_empty() {
                        // bcs representation of the empty vector
                        return Ok(vec![0]);
                    }
                    // write length of the vector as uleb128 as it is encoded in BCS and then append
                    // all (already serialized) object content data
                    let mut res = vec![];
                    leb128::write::unsigned(&mut res, vec.len() as u64).unwrap();
                    for arg in vec {
                        let object_kind = match arg {
                            ObjectArg::ImmOrOwnedObject(ref_) => {
                                InputObjectKind::ImmOrOwnedMoveObject(ref_)
                            }
                            ObjectArg::SharedObject {
                                id,
                                initial_shared_version,
                            } => InputObjectKind::SharedMoveObject {
                                id,
                                initial_shared_version,
                            },
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
                        type_check_struct(view, type_args, idx, arg_type, param_type)?;
                        res.extend(o);
                    }
                    res
                }
            };

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

// Validates a primitive argument
fn validate_primitive_arg(
    view: &BinaryIndexedView,
    arg: &[u8],
    idx: LocalIndex,
    param_type: &SignatureToken,
    type_layout: Option<MoveTypeLayout>,
) -> Result<(), ExecutionError> {
    // at this point we only check validity of string arguments (ascii and utf8)
    let string_arg_opt = string_arg(param_type, view);
    if string_arg_opt.is_none() {
        return Ok(());
    }
    let string_struct = string_arg_opt.unwrap();

    // we already checked the type above and struct layout for this type is guaranteed to exist
    let string_struct_layout = type_layout.unwrap();

    let string_move_value =
        MoveValue::simple_deserialize(arg, &string_struct_layout).map_err(|_| {
            ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                format!(
                "Function expects {}::{}::{} struct but provided argument's value does not match",
                string_struct.0, string_struct.1, string_struct.2,
            ),
            )
        })?;
    validate_string_move_value(&string_move_value, idx, string_struct.1)
}

// Given a MoveValue representing an string argument (string itself or arbitrarily nested vector of
// strings), validate that the content of the data represents a valid string
fn validate_string_move_value(
    value: &MoveValue,
    idx: LocalIndex,
    module: &IdentStr,
) -> Result<(), ExecutionError> {
    match value {
        MoveValue::Vector(vec) => {
            for v in vec {
                validate_string_move_value(v, idx, module)?;
            }
            Ok(())
        }
        MoveValue::Struct(MoveStruct::Runtime(vec)) => {
            // deserialization process validates the structure of this MoveValue (one struct field
            // in string structs containing a vector of u8 values)
            debug_assert!(vec.len() == 1);
            if let MoveValue::Vector(u8_vec) = &vec[0] {
                validate_string(&move_values_to_u8(u8_vec), idx, module)
            } else {
                debug_assert!(false);
                Ok(())
            }
        }
        _ => {
            debug_assert!(false);
            Ok(())
        }
    }
}

// Converts a Vec<MoveValue::U8> to a Vec<U8>
fn move_values_to_u8(values: &[MoveValue]) -> Vec<u8> {
    let res: Vec<u8> = values
        .iter()
        .filter_map(|v| {
            // deserialization process validates the structure of this MoveValue (u8 stored in a
            // vector of a string struct)
            if let MoveValue::U8(b) = v {
                Some(b)
            } else {
                None
            }
        })
        .cloned()
        .collect();
    debug_assert!(res.len() == values.len());
    res
}

// Validates that Vec<u8> represents a valid string
fn validate_string(bytes: &[u8], idx: LocalIndex, module: &IdentStr) -> Result<(), ExecutionError> {
    if module == STD_ASCII_MODULE_NAME {
        for b in bytes {
            if *b > 0x7F {
                return Err(ExecutionError::new_with_source(
                    ExecutionErrorKind::entry_argument_error(
                        idx,
                        EntryArgumentErrorKind::TypeMismatch,
                    ),
                    format!("Unexpected non-ASCII value (outside of ASCII character range) in argument {}", idx),
                ));
            }
        }
    } else {
        debug_assert!(module == STD_UTF8_MODULE_NAME);
        if std::str::from_utf8(bytes).is_err() {
            return Err(ExecutionError::new_with_source(
                ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
                format!(
                    "Unexpected non-UTF8 value (outside of UTF8 character range) in argument {}",
                    idx
                ),
            ));
        }
    }
    Ok(())
}

/// Check if a given argument is a string (or vector of strings) and, if so, return it
fn string_arg<'a>(
    param_type: &SignatureToken,
    view: &'a BinaryIndexedView,
) -> Option<(&'a AccountAddress, &'a IdentStr, &'a IdentStr)> {
    match param_type {
        SignatureToken::Struct(struct_handle_idx) => {
            let resolved_struct = sui_verifier::resolve_struct(view, *struct_handle_idx);
            if resolved_struct == RESOLVED_ASCII_STR || resolved_struct == RESOLVED_UTF8_STR {
                Some(resolved_struct)
            } else {
                None
            }
        }
        SignatureToken::Vector(el_token) => string_arg(el_token, view),
        _ => None,
    }
}

/// Serialize object with ID encoded in object_kind and also verify if various object properties are
/// correct.
fn serialize_object<'a>(
    object_kind: InputObjectKind,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    objects: &'a BTreeMap<ObjectID, impl Borrow<Object>>,
    object_data: &mut BTreeMap<ObjectID, (Owner, SequenceNumber)>,
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
        InputObjectKind::SharedMoveObject { .. } if !object.is_shared() => {
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
    object_data.insert(object_id, (object.owner, object.version()));
    Ok((
        move_object.contents().to_vec(),
        &move_object.type_,
        inner_param_type,
    ))
}

/// Get "inner" type of an object passed as argument (e.g., an inner type of a reference or of a
/// vector) and also verify if various object properties are correct.
fn inner_param_type<'a>(
    object: &Object,
    object_id: ObjectID,
    idx: LocalIndex,
    param_type: &'a SignatureToken,
    arg_type: &StructTag,
    mutable_ref_objects: &mut BTreeMap<u8, ObjectID>,
    by_value_objects: &mut BTreeSet<ObjectID>,
) -> Result<&'a SignatureToken, ExecutionError> {
    if let Owner::ObjectOwner(parent) = &object.owner {
        return Err(ExecutionErrorKind::invalid_child_object_argument(object_id, *parent).into());
    }
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
                Owner::Shared { .. } | Owner::Immutable => {
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
        t => Err(ExecutionError::new_with_source(
            ExecutionErrorKind::entry_argument_error(idx, EntryArgumentErrorKind::TypeMismatch),
            format!(
                "Found object argument {}, but function expects {:?}",
                arg_type, t
            ),
        )),
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
    _objects: &BTreeMap<ObjectID, impl Borrow<Object>>,
    _by_value_objects: &BTreeSet<ObjectID>,
    _object_type_map: &BTreeMap<ObjectID, ModuleId>,
    _current_module: ModuleId,
) -> Result<(), ExecutionError> {
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
