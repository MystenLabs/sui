// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use fastx_framework::natives;
use fastx_types::{
    base_types::{FastPayAddress, ObjectRef, SequenceNumber, TxContext},
    error::{FastPayError, FastPayResult},
    object::Object,
    storage::Storage,
    FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS,
};
use fastx_verifier::verifier;
use move_binary_format::{errors::VMError, file_format::CompiledModule};

use move_cli::sandbox::utils::get_gas_status;
use move_core_types::{
    account_address::AccountAddress,
    effects::ChangeSet,
    gas_schedule::GasAlgebra,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    resolver::{ModuleResolver, MoveResolver, ResourceResolver},
    transaction_argument::{convert_txn_args, TransactionArgument},
};
use move_vm_runtime::move_vm::MoveVM;
use std::{collections::BTreeMap, fmt::Debug};

use crate::bytecode_rewriter::ModuleHandleRewriter;

/// Execute `module::function<type_args>(object_args ++ pure_args)` as a call from `sender` with the given `gas_budget`.
/// Execution will read from/write to the store in `state_view`.
/// If `gas_budget` is None, runtime metering is disabled and execution may diverge.
#[allow(clippy::too_many_arguments)]
pub fn execute<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    module: &ModuleId,
    function: &Identifier,
    sender: AccountAddress,
    object_args: Vec<ObjectRef>,
    mut pure_args: Vec<Vec<u8>>,
    type_args: Vec<TypeTag>,
    gas_budget: Option<u64>,
) -> Result<()> {
    let obj_ids: Vec<TransactionArgument> = object_args
        .iter()
        .map(|o| TransactionArgument::Address(o.0))
        .collect();
    let mut args = convert_txn_args(&obj_ids);
    args.append(&mut pure_args);

    if let Err(error) = verify_module(module, state_view) {
        // TODO: execute should return Result<(), FastPayError>
        bail!("Verification error: {:?}", error)
    }
    match execute_function(
        module,
        function,
        type_args,
        vec![sender],
        pure_args,
        gas_budget,
        state_view,
    )? {
        ExecutionResult::Success {
            change_set,
            events,
            gas_used: _,
        } => {
            // process change set. important to do this before processing events because it's where deletions happen
            for (addr, addr_changes) in change_set.into_inner() {
                for (struct_tag, bytes_opt) in addr_changes.into_resources() {
                    match bytes_opt {
                        Some(bytes) => {
                            // object mutated during execution
                            let sequence_number = state_view
                                .read_object(&addr)
                                .ok_or(FastPayError::ObjectNotFound)?
                                .next_sequence_number
                                .increment()
                                .map_err(|_| FastPayError::InvalidSequenceNumber)?;

                            let owner = FastPayAddress::from_move_address_hack(&sender);
                            let object =
                                Object::new_move(struct_tag, bytes, owner, sequence_number);
                            state_view.write_object(object);
                        }
                        None => state_view.delete_object(&addr),
                    }
                }
            }
            // process events
            for e in events {
                if is_transfer_event(&e) {
                    let (guid, _seq_num, type_, event_bytes) = e;
                    match type_ {
                        TypeTag::Struct(s_type) => {
                            // special transfer event. process by saving object under given authenticator
                            let transferred_obj = event_bytes;
                            let recipient = AccountAddress::from_bytes(guid)?;
                            let sequence_number = SequenceNumber::new();
                            let owner = FastPayAddress::from_move_address_hack(&recipient);
                            let mut object =
                                Object::new_move(s_type, transferred_obj, owner, sequence_number);

                            // If object exists, find new sequence number
                            if let Some(old_object) = state_view.read_object(&object.id()) {
                                let sequence_number =
                                    old_object.next_sequence_number.increment()?;
                                object.next_sequence_number = sequence_number;
                            }

                            state_view.write_object(object);
                        }
                        _ => unreachable!("Only structs can be transferred"),
                    }
                } else {
                    // the fastX framework doesn't support user-generated events yet, so shouldn't hit this
                    unimplemented!("Processing user events")
                }
            }
        }
        ExecutionResult::Fail { error, gas_used: _ } => {
            bail!("Fail: {}", error)
        }
    }
    Ok(())
}

pub fn publish<E: Debug, S: ResourceResolver<Error = E> + ModuleResolver<Error = E> + Storage>(
    state_view: &mut S,
    module_bytes: Vec<Vec<u8>>,
    sender: &AccountAddress,
    ctx: &mut TxContext,
) -> Result<Vec<ObjectRef>, FastPayError> {
    if module_bytes.is_empty() {
        return Err(FastPayError::ModulePublishFailure {
            error: "Publishing empty list of modules".to_string(),
        });
    }

    let mut modules = module_bytes
        .iter()
        .map(|b| {
            CompiledModule::deserialize(b).map_err(|e| FastPayError::ModuleDeserializationFailure {
                error: e.to_string(),
            })
        })
        .collect::<FastPayResult<Vec<CompiledModule>>>()?;
    generate_module_ids(&mut modules, ctx)?;
    // verify and link modules, wrap them in objects, write them to the store
    let mut written_refs = Vec::with_capacity(modules.len());
    for module in modules {
        // It is important to do this before running the FastX verifier, since the fastX
        // verifier may assume well-formedness conditions enforced by the Move verifier hold
        move_bytecode_verifier::verify_module(&module).map_err(|e| {
            FastPayError::ModuleVerificationFailure {
                error: e.to_string(),
            }
        })?;
        // Run FastX bytecode verifier
        verifier::verify_module(&module)?;

        // TODO(https://github.com/MystenLabs/fastnft/issues/69):
        // run Move linker using state_view. it currently can only be called through the VM's publish or publish_module_bundle API's,
        // but we can't use those because they require module.self_address() == sender, which is not true for FastX modules
        let _ = state_view;

        // Create module objects and write them to the store
        let module_object = Object::new_module(
            module,
            FastPayAddress::from_move_address_hack(sender),
            SequenceNumber::new(),
        );
        written_refs.push(module_object.to_object_reference());
        state_view.write_object(module_object);
    }

    Ok(written_refs)
}

/// Use `ctx` to generate fresh ID's for each module in `modules`.
/// Mutate each module's self ID to the appropriate fresh ID and update its module handle tables
/// to reflect the new ID's of its dependencies
pub fn generate_module_ids(
    modules: &mut Vec<CompiledModule>,
    ctx: &mut TxContext,
) -> Result<(), FastPayError> {
    let mut sub_map = BTreeMap::new();
    for module in modules.iter() {
        // derive a fresh ID's for each module and mutate its self address to the ID.
        // this ensures that the  module can be uniquely identified/retrieved by its self address
        let old_module_id = module.self_id();
        let fresh_object_id = ctx.fresh_id();
        let new_module_id = ModuleId::new(fresh_object_id, old_module_id.name().to_owned());
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
    Ok(())
}

/// Check if this is a special event type emitted when there is a transfer between fastX addresses
pub fn is_transfer_event(e: &Event) -> bool {
    // TODO: hack that leverages implementation of Transfer::transfer_internal native function
    !e.0.is_empty()
}

// TODO: Code below here probably wants to move into the VM or elsewhere in
// the Diem codebase--seems generically useful + nothing similar exists

type Event = (Vec<u8>, u64, TypeTag, Vec<u8>);

/// Result of executing a script or script function in the VM
pub enum ExecutionResult {
    /// Execution completed successfully. Changes to global state are
    /// captured in `change_set`, and `events` are recorded in the order
    /// they were emitted. `gas_used` records the amount of gas expended
    /// by execution. Note that this will be 0 in unmetered mode.
    Success {
        change_set: ChangeSet,
        events: Vec<Event>,
        gas_used: u64,
    },
    /// Execution failed for the reason described in `error`.
    /// `gas_used` records the amount of gas expended by execution. Note
    /// that this will be 0 in unmetered mode.
    Fail { error: VMError, gas_used: u64 },
}

fn create_vm() -> MoveVM {
    let natives = natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);
    MoveVM::new(natives).expect("VM creation only fails if natives are invalid")
}

/// Execute the function named `script_function` in `module` with the given
/// `type_args`, `signer_addresses`, and `args` as input.
/// Execute the function according to the given `gas_budget`. If this budget
/// is `Some(t)`, use `t` use `t`; if None, run the VM in unmetered mode
/// Read published modules and global state from `resolver` and native functions
/// from `natives`.
#[allow(clippy::too_many_arguments)]
pub fn execute_function<Resolver: MoveResolver>(
    module: &ModuleId,
    script_function: &IdentStr,
    type_args: Vec<TypeTag>,
    signer_addresses: Vec<AccountAddress>,
    mut args: Vec<Vec<u8>>,
    gas_budget: Option<u64>,
    resolver: &Resolver,
) -> Result<ExecutionResult> {
    let vm = create_vm();
    let mut gas_status = get_gas_status(gas_budget)?;
    let mut session = vm.new_session(resolver);
    // prepend signers to args
    let mut signer_args: Vec<Vec<u8>> = signer_addresses
        .iter()
        .map(|s| bcs::to_bytes(s).unwrap())
        .collect();
    signer_args.append(&mut args);

    let res = {
        session
            .execute_function(
                module,
                script_function,
                type_args,
                signer_args,
                &mut gas_status,
            )
            .map(|_| ())
    };
    let gas_used = match gas_budget {
        Some(budget) => budget - gas_status.remaining_gas().get(),
        None => 0,
    };
    if let Err(error) = res {
        Ok(ExecutionResult::Fail { error, gas_used })
    } else {
        let (change_set, events) = session.finish().map_err(|e| e.into_vm_status())?;
        Ok(ExecutionResult::Success {
            change_set,
            events,
            gas_used,
        })
    }
}

// Load, deserialize, and check the module with the fastx bytecode verifier, without linking
fn verify_module<Resolver: MoveResolver>(id: &ModuleId, resolver: &Resolver) -> FastPayResult {
    let module_bytes = match resolver.get_module(id) {
        Ok(Some(bytes)) => bytes,
        Ok(None) => {
            return Err(FastPayError::ModuleLoadFailure {
                error: format!("Resolver returned None for module {}", &id),
            })
        }
        Err(err) => {
            return Err(FastPayError::ModuleLoadFailure {
                error: format!("Resolver failed to load module {}: {:?}", &id, err),
            })
        }
    };

    // for bytes obtained from the data store, they should always deserialize and verify.
    // It is an invariant violation if they don't.
    let module = CompiledModule::deserialize(&module_bytes).map_err(|err| {
        FastPayError::ModuleLoadFailure {
            error: err.to_string(),
        }
    })?;

    // bytecode verifier checks that can be performed with the module itself
    verifier::verify_module(&module).map_err(|err| FastPayError::ModuleVerificationFailure {
        error: err.to_string(),
    })?;
    Ok(())
}
