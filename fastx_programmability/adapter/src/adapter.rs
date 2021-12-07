// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use crate::{
    state_view::FastXStateView, swap_authenticator_and_id, FASTX_FRAMEWORK_ADDRESS,
    MOVE_STDLIB_ADDRESS,
};
use anyhow::Result;
use fastx_framework::natives;
use fastx_verifier::verifier;
use move_binary_format::{
    errors::{Location, PartialVMError, VMError, VMResult},
    file_format::CompiledModule,
};

use move_cli::sandbox::utils::get_gas_status;
use move_core_types::{
    account_address::AccountAddress,
    effects::ChangeSet,
    gas_schedule::GasAlgebra,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, TypeTag},
    resolver::MoveResolver,
    transaction_argument::{convert_txn_args, TransactionArgument},
    vm_status::StatusCode,
};
use move_vm_runtime::{move_vm::MoveVM, native_functions::NativeFunction};
use sha3::{Digest, Sha3_256};

pub struct FastXAdapter {
    state_view: FastXStateView,
}

impl FastXAdapter {
    pub fn create(build_dir: &str, storage_dir: &str) -> Result<Self> {
        let state_view = FastXStateView::create(build_dir, storage_dir)?;
        Ok(FastXAdapter { state_view })
    }

    /// Endpoint for local execution--no signature checking etc. is performed, and the result is saved on disk
    // TODO: implement a wrapper of this with tx prologue + epilogue, bytecode verifier passes, etc.
    pub fn execute_local(
        &mut self,
        module: Identifier,
        function: Identifier,
        sender: AccountAddress,
        mut args: Vec<TransactionArgument>,
        type_args: Vec<TypeTag>,
        gas_budget: Option<u64>,
    ) -> Result<()> {
        // calculate `inputs_hash` based on address arguments. each address is the identifier of an object accessed by `function`
        let mut hash_arg = Vec::new();
        for arg in &args {
            if let TransactionArgument::Address(a) = arg {
                hash_arg.append(&mut a.to_vec())
            }
        }
        // TODO: we should assert this eventually. but it makes testing difficult
        // because of bootstrapping--the initial state contains no objects :)
        //assert!(!hash_arg.is_empty(), "Need at least one object ID as input");
        let inputs_hash = Sha3_256::digest(&hash_arg);
        // assume that by convention, `inputs_hash` is the last argument
        args.push(TransactionArgument::U8Vector(inputs_hash.to_vec()));
        let script_args = convert_txn_args(&args);
        let module_id = ModuleId::new(FASTX_FRAMEWORK_ADDRESS, module);
        if let Err(error) = verify_module(&module_id, &self.state_view) {
            // TODO: use CLI's error explanation features here
            println!("Fail: {}", error);
            return Ok(());
        }
        let natives = natives::all_natives(MOVE_STDLIB_ADDRESS, FASTX_FRAMEWORK_ADDRESS);
        match execute_function(
            &module_id,
            &function,
            type_args,
            vec![sender],
            script_args,
            gas_budget,
            &self.state_view,
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
                            Some(bytes) => self
                                .state_view
                                .inner
                                .save_resource(addr, struct_tag, &bytes)?,
                            None => self.state_view.inner.delete_resource(addr, struct_tag)?,
                        }
                    }
                }
                // TODO: use CLI's explain_change_set here?
                // process events
                for e in events {
                    if Self::is_transfer_event(&e) {
                        let (guid, _seq_num, type_, event_bytes) = e;
                        match type_ {
                            TypeTag::Struct(s_type) => {
                                // special transfer event. process by saving object under given authenticator
                                let mut transferred_obj = event_bytes;
                                let recipient = AccountAddress::from_bytes(guid)?;
                                // hack: extract the ID from the object and use it as the address the object is saved under
                                // replace the id with the object's new owner `recipient`
                                let id = swap_authenticator_and_id(recipient, &mut transferred_obj);
                                self.state_view
                                    .inner
                                    .save_resource(id, s_type, &transferred_obj)?
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
                // TODO: use CLI's error explanation features here
                println!("Fail: {}", error)
            }
        }
        Ok(())
    }

    /// Check if this is a special event type emitted when there is a transfer between fastX addresses
    pub fn is_transfer_event(e: &Event) -> bool {
        // TODO: hack that leverages implementation of Transfer::transfer_internal native function
        !e.0.is_empty()
    }
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
        println!("Failure: {}", error);
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
fn verify_module<Resolver: MoveResolver>(id: &ModuleId, resolver: &Resolver) -> VMResult<()> {
    let module_bytes = match resolver.get_module(id) {
        Ok(Some(bytes)) => bytes,
        _ => {
            return Err(PartialVMError::new(StatusCode::LINKER_ERROR)
                .with_message(format!("Cannot find {:?} in data cache", id))
                .finish(Location::Undefined))
        }
    };

    // for bytes obtained from the data store, they should always deserialize and verify.
    // It is an invariant violation if they don't.
    let module = CompiledModule::deserialize(&module_bytes).map_err(|err| {
        let msg = format!("Deserialization error: {:?}", err);
        PartialVMError::new(StatusCode::CODE_DESERIALIZATION_ERROR)
            .with_message(msg)
            .finish(Location::Module(id.clone()))
    })?;

    // bytecode verifier checks that can be performed with the module itself
    verifier::verify_module(&module)?;
    Ok(())
}
