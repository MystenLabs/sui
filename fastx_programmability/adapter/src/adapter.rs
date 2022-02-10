// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use crate::bytecode_rewriter::ModuleHandleRewriter;
use fastx_framework::EventType;
use fastx_types::{
    base_types::{
        Authenticator, FastPayAddress, ObjectID, TransactionDigest, TxContext,
        TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME,
    },
    error::{FastPayError, FastPayResult},
    event::Event,
    gas,
    messages::ExecutionStatus,
    object::{Data, MoveObject, Object},
    storage::Storage,
    FASTX_FRAMEWORK_ADDRESS,
};
use fastx_verifier::verifier;
use move_binary_format::{
    errors::PartialVMResult,
    file_format::CompiledModule,
    normalized::{Function, Type},
};

use move_cli::sandbox::utils::get_gas_status;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    resolver::{ModuleResolver, ResourceResolver},
};
use move_vm_runtime::{native_functions::NativeFunctionTable, session::ExecutionResult};
use std::{
    borrow::Borrow,
    collections::{BTreeMap, HashMap},
    convert::TryFrom,
    fmt::Debug,
    sync::Arc,
};

pub use move_vm_runtime::move_vm::MoveVM;

macro_rules! exec_failure {
    ($gas:expr, $err:expr) => {
        return Ok(ExecutionStatus::Failure {
            gas_used: $gas,
            error: Box::new($err),
        })
    };
}

#[cfg(test)]
#[path = "unit_tests/adapter_tests.rs"]
mod adapter_tests;

pub fn new_move_vm(natives: NativeFunctionTable) -> Result<Arc<MoveVM>, FastPayError> {
    Ok(Arc::new(
        MoveVM::new(natives).map_err(|_| FastPayError::ExecutionInvariantViolation)?,
    ))
}

/// Execute `module::function<type_args>(object_args ++ pure_args)` as a call from `sender` with the given `gas_budget`.
/// Execution will read from/write to the store in `state_view`.
/// IMPORTANT NOTES on the return value:
/// The return value indicates whether a system error has occured (i.e. issues with the fastx system, not with user transaction).
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
    ctx: &TxContext,
) -> FastPayResult<ExecutionStatus> {
    let mut object_owner_map = HashMap::new();
    for object in object_args.iter().filter(|obj| !obj.is_read_only()) {
        if let Authenticator::Object(owner_object_id) = object.owner {
            object_owner_map.insert(object.id(), owner_object_id);
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
        ctx,
    ) {
        Ok(ok) => ok,
        Err(err) => {
            exec_failure!(gas::MIN_MOVE_CALL_GAS, err);
        }
    };

    // TODO: Update Move gas constants to reflect the gas fee on fastx.
    let cost_table = &move_vm_types::gas_schedule::INITIAL_COST_SCHEDULE;
    let mut gas_status = match get_gas_status(cost_table, Some(gas_budget)).map_err(|e| {
        FastPayError::GasBudgetTooHigh {
            error: e.to_string(),
        }
    }) {
        Ok(ok) => ok,
        Err(err) => {
            exec_failure!(gas::MIN_MOVE_CALL_GAS, err);
        }
    };
    let session = vm.new_session(state_view);
    match session.execute_function_for_effects(
        &module_id,
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
            // FastX Move programs should never touch global state, so ChangeSet should be empty
            debug_assert!(change_set.accounts().is_empty());
            // Input ref parameters we put in should be the same number we get out, plus one for the &mut TxContext
            debug_assert!(mutable_ref_objects.len() + 1 == mutable_ref_values.len());
            debug_assert!(gas_used <= gas_budget);
            // discard the &mut TxContext arg
            mutable_ref_values.pop().unwrap();
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
                exec_failure!(std::cmp::min(total_gas, gas_budget), err);
            }
            match gas::try_deduct_gas(&mut gas_object, total_gas) {
                Ok(()) => {
                    state_view.write_object(gas_object);
                    Ok(ExecutionStatus::Success)
                }
                Err(err) => exec_failure!(gas_budget, err),
            }
        }
        ExecutionResult::Fail { error, gas_used } => exec_failure!(
            gas_used,
            FastPayError::AbortedExecution {
                error: error.to_string(),
            }
        ),
    }
}

/// Similar to execute(), only returns Err if there are system issues.
/// ExecutionStatus contains the actual execution result.
pub fn publish<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    module_bytes: Vec<Vec<u8>>,
    sender: FastPayAddress,
    ctx: &mut TxContext,
    mut gas_object: Object,
) -> FastPayResult<ExecutionStatus> {
    // Deduct gas upfront, if not enough balance, bail out early.
    let gas_cost = gas::calculate_module_publish_cost(&module_bytes);
    if let Err(err) = gas::try_deduct_gas(&mut gas_object, gas_cost) {
        exec_failure!(gas::MIN_MOVE_PUBLISH_GAS, err);
    }
    state_view.write_object(gas_object);

    let result = module_bytes
        .iter()
        .map(|b| CompiledModule::deserialize(b))
        .collect::<PartialVMResult<Vec<CompiledModule>>>();
    let mut modules = match result {
        Ok(ok) => ok,
        Err(err) => {
            exec_failure!(
                gas::MIN_MOVE_PUBLISH_GAS,
                FastPayError::ModuleDeserializationFailure {
                    error: err.to_string(),
                }
            );
        }
    };

    // run validation checks
    if modules.is_empty() {
        exec_failure!(
            gas::MIN_MOVE_PUBLISH_GAS,
            FastPayError::ModulePublishFailure {
                error: "Publishing empty list of modules".to_string(),
            }
        );
    }
    let package_id = match generate_package_id(&mut modules, ctx) {
        Ok(ok) => ok,
        Err(err) => exec_failure!(gas::MIN_MOVE_PUBLISH_GAS, err),
    };
    if let Err(err) = verify_and_link(state_view, &modules, package_id, natives) {
        exec_failure!(gas::MIN_MOVE_PUBLISH_GAS, err);
    }

    // wrap the modules in an object, write it to the store
    let package_object = Object::new_package(modules, Authenticator::Address(sender), ctx.digest());
    state_view.write_object(package_object);

    Ok(ExecutionStatus::Success)
}

/// Given a list of `modules`, links each module against its
/// dependencies and runs each module with both the Move VM verifier
/// and the FastX verifier.
pub fn verify_and_link<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    state_view: &S,
    modules: &[CompiledModule],
    package_id: ObjectID,
    natives: NativeFunctionTable,
) -> Result<(), FastPayError> {
    // Run the Move bytecode verifier and linker.
    // It is important to do this before running the FastX verifier, since the fastX
    // verifier may assume well-formedness conditions enforced by the Move verifier hold
    let vm = MoveVM::new(natives)
        .expect("VM creation only fails if natives are invalid, and we created the natives");
    // Note: VM does not do any gas metering on publish code path, so setting budget to None is fine
    let cost_table = &move_vm_types::gas_schedule::INITIAL_COST_SCHEDULE;
    let mut gas_status = get_gas_status(cost_table, None)
        .expect("Can only fail if gas budget is too high, and we didn't supply one");
    let mut session = vm.new_session(state_view);
    // TODO(https://github.com/MystenLabs/fastnft/issues/69): avoid this redundant serialization by exposing VM API that allows us to run the linker directly on `Vec<CompiledModule>`
    let new_module_bytes = modules
        .iter()
        .map(|m| {
            let mut bytes = Vec::new();
            m.serialize(&mut bytes).unwrap();
            bytes
        })
        .collect();
    session
        .publish_module_bundle(new_module_bytes, package_id, &mut gas_status)
        .map_err(|e| FastPayError::ModulePublishFailure {
            error: e.to_string(),
        })?;

    // run the FastX verifier
    for module in modules.iter() {
        // Run FastX bytecode verifier, which runs some additional checks that assume the Move bytecode verifier has passed.
        verifier::verify_module(module)?;
    }
    Ok(())
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
) -> Result<ObjectID, FastPayError> {
    let mut sub_map = BTreeMap::new();
    let package_id = ctx.fresh_id();
    for module in modules.iter() {
        let old_module_id = module.self_id();
        let old_address = *old_module_id.address();
        if old_address != AccountAddress::ZERO {
            return Err(FastPayError::ModulePublishFailure {
                error: "Publishing modules with non-zero address is not allowed".to_string(),
            });
        }
        let new_module_id = ModuleId::new(package_id, old_module_id.name().to_owned());
        if sub_map.insert(old_module_id, new_module_id).is_some() {
            return Err(FastPayError::ModulePublishFailure {
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
/// - Returns (amount of extra gas used, amount of gas refund)
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
    mut object_owner_map: HashMap<ObjectID, ObjectID>,
) -> (u64, u64, FastPayResult) {
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
    for e in events {
        let (recipient, event_type, type_, event_bytes) = e;
        let result = match EventType::try_from(event_type as u8)
            .expect("Safe because event_type is derived from an EventType enum")
        {
            EventType::TransferToAddress => handle_transfer(
                Authenticator::Address(FastPayAddress::try_from(recipient.borrow()).unwrap()),
                type_,
                event_bytes,
                false, /* should_freeze */
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::TransferToAddressAndFreeze => handle_transfer(
                Authenticator::Address(FastPayAddress::try_from(recipient.borrow()).unwrap()),
                type_,
                event_bytes,
                true, /* should_freeze */
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::TransferToObject => handle_transfer(
                Authenticator::Object(ObjectID::try_from(recipient.borrow()).unwrap()),
                type_,
                event_bytes,
                false, /* should_freeze */
                tx_digest,
                &mut by_value_objects,
                &mut gas_used,
                state_view,
                &mut object_owner_map,
            ),
            EventType::DeleteObjectID => {
                // TODO: Process deleted object event.
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
    // this means that either the object was (1) deleted from the FastX system altogether, or
    // (2) wrapped inside another object that is in the FastX object pool
    // in either case, we want to delete it
    let mut gas_refund: u64 = 0;
    for (id, object) in by_value_objects.iter() {
        state_view.delete_object(id);
        gas_refund += gas::calculate_object_deletion_refund(object);
    }

    (gas_used, gas_refund, Ok(()))
}

#[allow(clippy::too_many_arguments)]
fn handle_transfer<
    E: Debug,
    S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage,
>(
    recipient: Authenticator,
    type_: TypeTag,
    contents: Vec<u8>,
    should_freeze: bool,
    tx_digest: TransactionDigest,
    by_value_objects: &mut BTreeMap<ObjectID, Object>,
    gas_used: &mut u64,
    state_view: &mut S,
    object_owner_map: &mut HashMap<ObjectID, ObjectID>,
) -> FastPayResult {
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
            if should_freeze {
                move_obj.freeze();
            }
            let obj = Object::new_move(move_obj, recipient, tx_digest);
            if old_object.is_none() {
                // Charge extra gas based on object size if we are creating a new object.
                *gas_used += gas::calculate_object_creation_cost(&obj);
            }
            let obj_id = obj.id();
            // Below we check whether the transfer introduced any circular ownership.
            // We know that for any mutable object, all its ancenstors (if it was owned by another object)
            // must be in the input as well. Prior to this we have recored the original ownership mapping
            // in object_owner_map. For any new transfer, we trace the new owner through the ownership
            // chain to see if a cycle is detected.
            // TODO: Set a constant upper bound to the depth of the new ownership chain.
            object_owner_map.remove(&obj_id);
            if let Authenticator::Object(owner_object_id) = recipient {
                let mut parent = owner_object_id;
                while parent != obj_id && object_owner_map.contains_key(&parent) {
                    parent = *object_owner_map.get(&parent).unwrap();
                }
                if parent == obj_id {
                    return Err(FastPayError::CircularObjectOwnership);
                }
                object_owner_map.insert(obj_id, owner_object_id);
            }

            state_view.write_object(obj);
        }
        _ => unreachable!("Only structs can be transferred"),
    }
    Ok(())
}

struct TypeCheckSuccess {
    module_id: ModuleId,
    args: Vec<Vec<u8>>,
    by_value_objects: BTreeMap<ObjectID, Object>,
    mutable_ref_objects: Vec<Object>,
}

/// - Check that `package_object`, `module` and `function` are valid
/// - Check that the the signature of `function` is well-typed w.r.t `type_args`, `object_args`, and `pure_args`
/// - Return the ID of the resolved module, a vector of BCS encoded arguments to pass to the VM, and a partitioning
/// of the input objects into objects passed by value vs by mutable reference
fn resolve_and_type_check(
    package_object: Object,
    module: &Identifier,
    function: &Identifier,
    type_args: &[TypeTag],
    object_args: Vec<Object>,
    mut pure_args: Vec<Vec<u8>>,
    ctx: &TxContext,
) -> Result<TypeCheckSuccess, FastPayError> {
    // resolve the function we are calling
    let (function_signature, module_id) = match package_object.data {
        Data::Package(modules) => {
            let bytes = modules
                .get(module.as_str())
                .ok_or(FastPayError::ModuleNotFound {
                    module_name: module.to_string(),
                })?;
            let m = CompiledModule::deserialize(bytes).expect(
                "Unwrap safe because FastX serializes/verifies modules before publishing them",
            );
            (
                Function::new_from_name(&m, function).ok_or(FastPayError::FunctionNotFound {
                    error: format!(
                        "Could not resolve function '{}' in module {}",
                        function,
                        m.self_id()
                    ),
                })?,
                m.self_id(),
            )
        }
        Data::Move(_) => {
            return Err(FastPayError::ModuleLoadFailure {
                error: "Expected a module object, but found a Move object".to_string(),
            })
        }
    };
    // check validity conditions on the invoked function
    if !function_signature.return_.is_empty() {
        return Err(FastPayError::InvalidFunctionSignature {
            error: "Invoked function must not return a value".to_string(),
        });
    }
    // check arity of type and value arguments
    if function_signature.type_parameters.len() != type_args.len() {
        return Err(FastPayError::InvalidFunctionSignature {
            error: format!(
                "Expected {:?} type arguments, but found {:?}",
                function_signature.type_parameters.len(),
                type_args.len()
            ),
        });
    }
    // total number of args is |objects| + |pure_args| + 1 for the the `TxContext` object
    let num_args = object_args.len() + pure_args.len() + 1;
    if function_signature.parameters.len() != num_args {
        return Err(FastPayError::InvalidFunctionSignature {
            error: format!(
                "Expected {:?} arguments calling function '{}', but found {:?}",
                function_signature.parameters.len(),
                function,
                num_args
            ),
        });
    }
    // check that the last arg is `&mut TxContext`
    if let Type::MutableReference(s) =
        &function_signature.parameters[function_signature.parameters.len() - 1]
    {
        // TODO: does Rust let you pattern match on a nested box? can simplify big time if so...
        match s.borrow() {
            Type::Struct {
            address,
            module,
            name,
            type_arguments,
        } if address == &FASTX_FRAMEWORK_ADDRESS
            && module.as_ident_str() == TX_CONTEXT_MODULE_NAME
            && name.as_ident_str() == TX_CONTEXT_STRUCT_NAME
            && type_arguments.is_empty() => {}
        t => {
            return Err(FastPayError::InvalidFunctionSignature {
                error: format!(
                    "Expected last parameter of function signature to be &mut {}::{}::{}, but found {}",
                    FASTX_FRAMEWORK_ADDRESS, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME, t
                ),
            })
        }
    }
    } else {
        return Err(FastPayError::InvalidFunctionSignature {
            error: format!(
                "Expected last parameter of function signature to be &mut {}::{}::{}, but found non-reference_type",
                FASTX_FRAMEWORK_ADDRESS, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME
            ),
        });
    }

    // type check object arguments passed in by value and by reference
    let mut args = Vec::new();
    let mut mutable_ref_objects = Vec::new();
    let mut by_value_objects = BTreeMap::new();
    #[cfg(debug_assertions)]
    let mut num_immutable_objects = 0;
    #[cfg(debug_assertions)]
    let num_objects = object_args.len();

    let ty_args: Vec<Type> = type_args.iter().map(|t| Type::from(t.clone())).collect();
    for (idx, object) in object_args.into_iter().enumerate() {
        let mut param_type = function_signature.parameters[idx].clone();
        if !param_type.is_closed() {
            param_type = param_type.subst(&ty_args);
        }
        match &object.data {
            Data::Move(m) => {
                args.push(m.contents().to_vec());
                // check that m.type_ matches the parameter types of the function
                match &param_type {
                    Type::MutableReference(inner_t) => {
                        if m.is_read_only() {
                            return Err(FastPayError::TypeError {
                                error: format!(
                                    "Argument {} is expected to be mutable, immutable object found",
                                    idx
                                ),
                            });
                        }
                        type_check_struct(&m.type_, inner_t)?;
                        mutable_ref_objects.push(object);
                    }
                    Type::Reference(inner_t) => {
                        type_check_struct(&m.type_, inner_t)?;
                        #[cfg(debug_assertions)]
                        {
                            num_immutable_objects += 1
                        }
                    }
                    Type::Struct { .. } => {
                        if m.is_read_only() {
                            return Err(FastPayError::TypeError {
                                error: format!(
                                    "Argument {} is expected to be mutable, immutable object found",
                                    idx
                                ),
                            });
                        }
                        type_check_struct(&m.type_, &param_type)?;
                        let res = by_value_objects.insert(object.id(), object);
                        // should always pass due to earlier "no duplicate ID's" check
                        debug_assert!(res.is_none())
                    }
                    t => {
                        return Err(FastPayError::TypeError {
                            error: format!(
                                "Found object argument {}, but function expects {}",
                                m.type_, t
                            ),
                        })
                    }
                }
            }
            Data::Package(_) => {
                return Err(FastPayError::TypeError {
                    error: format!("Found module argument, but function expects {}", param_type),
                })
            }
        }
    }
    #[cfg(debug_assertions)]
    debug_assert!(
        by_value_objects.len() + mutable_ref_objects.len() + num_immutable_objects == num_objects
    );
    // check that the non-object parameters are primitive types
    for param_type in
        &function_signature.parameters[args.len()..function_signature.parameters.len() - 1]
    {
        if !is_primitive(param_type) {
            return Err(FastPayError::TypeError {
                error: format!("Expected primitive type, but found {}", param_type),
            });
        }
    }
    args.append(&mut pure_args);
    args.push(ctx.to_vec());

    Ok(TypeCheckSuccess {
        module_id,
        args,
        by_value_objects,
        mutable_ref_objects,
    })
}

fn type_check_struct(arg_type: &StructTag, param_type: &Type) -> Result<(), FastPayError> {
    if let Some(param_struct_type) = param_type.clone().into_struct_tag() {
        if arg_type != &param_struct_type {
            Err(FastPayError::TypeError {
                error: format!(
                    "Expected argument of type {}, but found type {}",
                    param_struct_type, arg_type
                ),
            })
        } else {
            Ok(())
        }
    } else {
        Err(FastPayError::TypeError {
            error: format!(
                "Expected argument of type {}, but found struct type {}",
                param_type, arg_type
            ),
        })
    }
}

// TODO: upstream Type::is_primitive in diem
fn is_primitive(t: &Type) -> bool {
    use Type::*;
    match t {
        Bool | U8 | U64 | U128 | Address => true,
        Vector(inner_t) => is_primitive(inner_t),
        Signer | Struct { .. } | TypeParameter(_) | Reference(_) | MutableReference(_) => false,
    }
}

#[cfg(debug_assertions)]
fn check_transferred_object_invariants(new_object: &MoveObject, old_object: &Option<Object>) {
    if let Some(o) = old_object {
        // check consistency between the transferred object `new_object` and the tx input `o`
        // specificially, the object id, type, and version should be unchanged
        let m = o.data.try_as_move().unwrap();
        debug_assert_eq!(m.id(), new_object.id());
        debug_assert_eq!(m.version(), new_object.version());
        debug_assert_eq!(m.type_, new_object.type_);
    }
}
