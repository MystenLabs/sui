// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    NativeFunctionRecord,
    sandbox::utils::{
        contains_module, explain_execution_error, get_gas_status, is_bytecode_file,
        on_disk_state_view::OnDiskStateView,
    },
};
use anyhow::{Result, anyhow, bail};
use move_binary_format::file_format::CompiledModule;
use move_command_line_common::files::try_exists;
use move_core_types::{
    account_address::AccountAddress, identifier::IdentStr, language_storage::TypeTag,
    runtime_value::MoveValue,
};
use move_package_alt_compilation::compiled_package::CompiledPackage;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{fs, path::Path};

pub fn run(
    natives: impl IntoIterator<Item = NativeFunctionRecord>,
    cost_table: &CostTable,
    state: &OnDiskStateView,
    _package: &CompiledPackage,
    module_file: &Path,
    function_name: &str,
    signers: &[String],
    txn_args: &[MoveValue],
    vm_type_tags: Vec<TypeTag>,
    gas_budget: Option<u64>,
    _dry_run: bool,
    _verbose: bool,
) -> Result<()> {
    if !try_exists(module_file)? {
        bail!("Module file {:?} does not exist", module_file)
    };
    assert!(
        is_bytecode_file(module_file)
            && (state.is_module_path(module_file) || !contains_module(module_file)),
        "Attempting to run module {:?} outside of the `storage/` directory.
        move run` must be applied to a module inside `storage/`",
        module_file
    );
    let bytecode = fs::read(module_file)?;

    let signer_addresses = signers
        .iter()
        .map(|s| AccountAddress::from_hex_literal(s))
        .collect::<Result<Vec<AccountAddress>, _>>()?;
    let vm_args: Vec<Vec<u8>> = txn_args
        .iter()
        .map(|arg| {
            arg.simple_serialize()
                .expect("Transaction arguments must serialize")
        })
        .collect();

    let vm = MoveVM::new(natives).unwrap();
    let mut gas_status = get_gas_status(cost_table, gas_budget)?;
    let mut session = vm.new_session(state);

    let script_type_parameters = vec![];
    let script_parameters = vec![];

    let script_type_arguments = vm_type_tags
        .iter()
        .map(|tag| session.load_type(tag))
        .collect::<Result<Vec<_>, _>>()?;

    // TODO rethink move-cli arguments for executing functions
    let vm_args = signer_addresses
        .iter()
        .map(|a| {
            MoveValue::Signer(*a)
                .simple_serialize()
                .expect("transaction arguments must serialize")
        })
        .chain(vm_args)
        .collect();
    let res = {
        // script fun. parse module, extract script ID to pass to VM
        let module = CompiledModule::deserialize_with_defaults(&bytecode)
            .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
        session.execute_entry_function(
            &module.self_id(),
            IdentStr::new(function_name)?,
            script_type_arguments,
            vm_args,
            &mut gas_status,
        )
    };

    if let Err(err) = res {
        explain_execution_error(
            err,
            state,
            &script_type_parameters,
            &script_parameters,
            &vm_type_tags,
            &signer_addresses,
            txn_args,
        )
    } else {
        let _changeset = session.finish().0?;
        Ok(())
    }
}
