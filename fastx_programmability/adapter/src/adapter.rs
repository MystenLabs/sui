// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use anyhow::{bail, Result};
use fastx_framework::{natives, FASTX_FRAMEWORK_ADDRESS, MOVE_STDLIB_ADDRESS};
use fastx_types::{
    base_types::{FastPayAddress, ObjectRef, SequenceNumber},
    error::{FastPayError, FastPayResult},
    object::Object,
    storage::Storage,
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
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunction};
use std::fmt::Debug;

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
    let natives = natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);
    match execute_function(
        module,
        function,
        type_args,
        vec![sender],
        pure_args,
        gas_budget,
        state_view,
        natives,
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
                            // TODO (https://github.com/MystenLabs/fastnft/issues/30):
                            // eventually, a mutation will only happen to an objects passed as a &mut input to the `main`, so we'll know
                            // its old sequence number. for now, we fake it.
                            let sequence_number = SequenceNumber::new();
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
                            // TODO (https://github.com/MystenLabs/fastnft/issues/30):
                            // eventually , a transfer will only happen to an objects passed as an owned input to the `main` (in which
                            // case we'll know its old sequence number), *or* it will be be freshly created (in which case its sequence #
                            // will be zero)
                            let sequence_number = SequenceNumber::new();
                            let owner = FastPayAddress::from_move_address_hack(&recipient);
                            let object =
                                Object::new_move(s_type, transferred_obj, owner, sequence_number);
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
    natives: impl IntoIterator<Item = (AccountAddress, Identifier, Identifier, NativeFunction)>,
) -> Result<ExecutionResult> {
    let vm = MoveVM::new(natives).unwrap();
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
