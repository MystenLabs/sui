// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;

use crate::bytecode_rewriter::ModuleHandleRewriter;
use fastx_types::{
    base_types::{
        FastPayAddress, ObjectID, TxContext, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME,
    },
    error::{FastPayError, FastPayResult},
    gas::{
        calculate_module_publish_cost, calculate_object_creation_cost,
        calculate_object_deletion_refund, deduct_gas,
    },
    object::{Data, MoveObject, Object},
    storage::Storage,
    FASTX_FRAMEWORK_OBJECT_ID,
};
use fastx_verifier::verifier;
use move_binary_format::{
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
use move_vm_runtime::{
    move_vm::MoveVM, native_functions::NativeFunctionTable, session::ExecutionResult,
};
use std::{borrow::Borrow, collections::BTreeMap, convert::TryFrom, fmt::Debug};

#[cfg(test)]
#[path = "unit_tests/adapter_tests.rs"]
mod adapter_tests;

/// Execute `module::function<type_args>(object_args ++ pure_args)` as a call from `sender` with the given `gas_budget`.
/// Execution will read from/write to the store in `state_view`.
/// If `gas_budget` is None, runtime metering is disabled and execution may diverge.
#[allow(clippy::too_many_arguments)]
pub fn execute<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    natives: NativeFunctionTable,
    package_object: Object,
    module: &Identifier,
    function: &Identifier,
    type_args: Vec<TypeTag>,
    object_args: Vec<Object>,
    pure_args: Vec<Vec<u8>>,
    gas_budget: u64,
    mut gas_object: Object,
    ctx: TxContext,
) -> Result<(), FastPayError> {
    let TypeCheckSuccess {
        module_id,
        args,
        mutable_ref_objects,
        by_value_objects,
    } = resolve_and_type_check(
        package_object,
        module,
        function,
        &type_args,
        object_args,
        pure_args,
        &ctx,
    )?;

    let vm = MoveVM::new(natives)
        .expect("VM creation only fails if natives are invalid, and we created the natives");
    // TODO: Update Move gas constants to reflect the gas fee on fastx.
    let mut gas_status =
        get_gas_status(Some(gas_budget)).map_err(|e| FastPayError::GasBudgetTooHigh {
            error: e.to_string(),
        })?;
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
            // discard the &mut TxContext arg
            mutable_ref_values.pop().unwrap();
            let mutable_refs = mutable_ref_objects
                .into_iter()
                .zip(mutable_ref_values.into_iter());
            process_successful_execution(
                state_view,
                by_value_objects,
                mutable_refs,
                events,
                gas_object,
                gas_used,
                gas_budget,
                &ctx,
            )?;

            Ok(())
        }
        ExecutionResult::Fail { error, gas_used } => {
            // Need to deduct gas even if the execution failed.
            deduct_gas(&mut gas_object, gas_used as i128)?;
            state_view.write_object(gas_object);
            // TODO: Keep track the gas deducted so that we could give them to participants.

            Err(FastPayError::AbortedExecution {
                error: error.to_string(),
            })
        }
    }
}

pub fn publish<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    module_bytes: Vec<Vec<u8>>,
    sender: FastPayAddress,
    ctx: &mut TxContext,
    mut gas_object: Object,
) -> Result<(), FastPayError> {
    if module_bytes.is_empty() {
        return Err(FastPayError::ModulePublishFailure {
            error: "Publishing empty list of modules".to_string(),
        });
    }
    let gas_cost = calculate_module_publish_cost(&module_bytes);
    deduct_gas(&mut gas_object, gas_cost as i128)?;
    state_view.write_object(gas_object);
    // TODO: Keep track the gas deducted so that we could give them to participants.

    let modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize(b).map_err(|e| FastPayError::ModuleDeserializationFailure {
                error: e.to_string(),
            })
        })
        .collect::<FastPayResult<Vec<CompiledModule>>>()?;
    let packages = generate_package_info_map(modules, ctx)?;
    // verify and link modules, wrap them in objects, write them to the store
    for (_, modules) in packages.into_values() {
        for module in modules.iter() {
            // It is important to do this before running the FastX verifier, since the fastX
            // verifier may assume well-formedness conditions enforced by the Move verifier hold
            move_bytecode_verifier::verify_module(module).map_err(|e| {
                FastPayError::ModuleVerificationFailure {
                    error: e.to_string(),
                }
            })?;
            // Run FastX bytecode verifier
            verifier::verify_module(module)?;

            // TODO(https://github.com/MystenLabs/fastnft/issues/69):
            // run Move linker using state_view. it currently can only be called through the VM's publish or publish_module_bundle API's,
            // but we can't use those because they require module.self_address() == sender, which is not true for FastX modules
        }
        let package_object = Object::new_package(modules, sender, ctx.digest());
        state_view.write_object(package_object);
    }

    Ok(())
}

/// Given a list of `modules`, regonize the packages from them (i.e. modules with the same address),
/// use `ctx` to generate a fresh ID for each of those packages.
/// Mutate each module's self ID to the appropriate fresh ID and update its module handle tables
/// to reflect the new ID's of its dependencies.
/// Returns the mapping from the old package addresses to the new addresses.
/// Note: id and address means the same thing here.
pub fn generate_package_info_map(
    modules: Vec<CompiledModule>,
    ctx: &mut TxContext,
) -> Result<BTreeMap<AccountAddress, (AccountAddress, Vec<CompiledModule>)>, FastPayError> {
    let mut sub_map = BTreeMap::new();
    let mut packages = BTreeMap::new();
    for module in modules {
        let old_module_id = module.self_id();
        let old_address = *old_module_id.address();
        let package_info = packages
            .entry(old_address)
            .or_insert((ctx.fresh_id(), vec![]));
        package_info.1.push(module);
        let new_module_id = ModuleId::new(package_info.0, old_module_id.name().to_owned());
        if sub_map.insert(old_module_id, new_module_id).is_some() {
            return Err(FastPayError::ModulePublishFailure {
                error: "Publishing two modules with the same ID".to_string(),
            });
        }
    }

    // Safe to unwrap because we checked for duplicate domain entries above, and range entries are fresh ID's
    let rewriter = ModuleHandleRewriter::new(sub_map).unwrap();
    for (_, modules) in packages.values_mut() {
        for module in modules.iter_mut() {
            // rewrite module handles to reflect freshly generated ID's
            rewriter.sub_module_ids(module);
        }
    }
    Ok(packages)
}

type Event = (Vec<u8>, u64, TypeTag, Vec<u8>);

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
    mut by_value_objects: BTreeMap<ObjectID, Object>,
    mutable_refs: impl Iterator<Item = (Object, Vec<u8>)>,
    events: Vec<Event>,
    mut gas_object: Object,
    mut gas_used: u64,
    gas_budget: u64,
    ctx: &TxContext,
) -> Result<(), FastPayError> {
    for (mut obj, new_contents) in mutable_refs {
        // update contents and increment sequence number
        obj.data
            .try_as_move_mut()
            .expect("We previously checked that mutable ref inputs are Move objects")
            .update_contents(new_contents)?;
        state_view.write_object(obj);
    }
    // process events to identify transfers, freezes
    for e in events {
        let (recipient, should_freeze, type_, event_bytes) = e;
        debug_assert!(!recipient.is_empty() && should_freeze < 2);
        match type_ {
            TypeTag::Struct(s_type) => {
                let contents = event_bytes;
                let should_freeze = should_freeze != 0;
                // unwrap safe due to size enforcement in Move code for `Authenticator
                let recipient = FastPayAddress::try_from(recipient.borrow()).unwrap();
                let mut move_obj = MoveObject::new(s_type, contents);
                let old_object = by_value_objects.remove(&move_obj.id());

                #[cfg(debug_assertions)]
                {
                    check_transferred_object_invariants(&move_obj, &old_object)
                }

                // increment the object version. note that if the transferred object was
                // freshly created, this means that its version will now be 1.
                // thus, all objects in the global object pool have version > 0
                move_obj.increment_version()?;
                if should_freeze {
                    move_obj.freeze();
                }
                let obj = Object::new_move(move_obj, recipient, ctx.digest());
                if old_object.is_none() {
                    // Charge extra gas based on object size if we are creating a new object.
                    gas_used += calculate_object_creation_cost(&obj);
                }
                state_view.write_object(obj);
            }
            _ => unreachable!("Only structs can be transferred"),
        }
    }
    if gas_used > gas_budget {
        return Err(FastPayError::InsufficientGas {
            error: format!(
                "Gas budget is {}, not enough to pay for cost {}",
                gas_budget, gas_used
            ),
        });
    }
    // any object left in `by_value_objects` is an input passed by value that was not transferred or frozen.
    // this means that either the object was (1) deleted from the FastX system altogether, or
    // (2) wrapped inside another object that is in the FastX object pool
    // in either case, we want to delete it
    let mut gas_refund: u64 = 0;
    for (id, object) in by_value_objects.iter() {
        state_view.delete_object(id);
        gas_refund += calculate_object_deletion_refund(object);
    }

    // TODO: In the current approach, we basically can use refund gas to pay for current transaction.
    // Is this allowed?
    deduct_gas(&mut gas_object, (gas_used as i128) - (gas_refund as i128))?;
    state_view.write_object(gas_object);
    // TODO: Keep track the gas deducted so that we could give them to participants.

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
                        "Could not resolve function {} in module {}",
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
    if !function_signature
        .parameters
        .iter()
        .all(|ty| ty.is_closed())
    {
        return Err(FastPayError::InvalidFunctionSignature {
            error: "Invoked function must not have an unbound type parameter".to_string(),
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
                "Expected {:?} arguments, but found {:?}",
                function_signature.parameters.len(),
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
        } if address == &FASTX_FRAMEWORK_OBJECT_ID
            && module.as_ident_str() == TX_CONTEXT_MODULE_NAME
            && name.as_ident_str() == TX_CONTEXT_STRUCT_NAME
            && type_arguments.is_empty() => {}
        t => {
            return Err(FastPayError::InvalidFunctionSignature {
                error: format!(
                    "Expected last parameter of function signature to be &mut {}::{}::{}, but found {}",
                    FASTX_FRAMEWORK_OBJECT_ID, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME, t
                ),
            })
        }
    }
    } else {
        return Err(FastPayError::InvalidFunctionSignature {
            error: format!(
                "Expected last parameter of function signature to be &mut {}::{}::{}, but found non-reference_type",
                FASTX_FRAMEWORK_OBJECT_ID, TX_CONTEXT_MODULE_NAME, TX_CONTEXT_STRUCT_NAME
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
    args.push(ctx.to_bcs_bytes_hack());

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
