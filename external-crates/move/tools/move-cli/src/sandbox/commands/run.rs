// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sandbox::utils::{
        contains_module, explain_execution_effects, explain_execution_error, get_gas_status,
        is_bytecode_file, maybe_commit_effects, on_disk_state_view::OnDiskStateView,
    },
    NativeFunctionRecord,
};
use anyhow::{anyhow, bail, Result};
use move_binary_format::file_format::CompiledModule;
use move_command_line_common::{env::get_bytecode_version_from_env, files::try_exists};
use move_core_types::{
    account_address::AccountAddress,
    errmap::ErrorMapping,
    identifier::IdentStr,
    language_storage::TypeTag,
    transaction_argument::{convert_txn_args, TransactionArgument},
    value::MoveValue,
};
use move_package::compilation::compiled_package::CompiledPackage;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::gas_schedule::CostTable;
use std::{fs, path::Path};

pub fn run(
    natives: impl IntoIterator<Item = NativeFunctionRecord>,
    cost_table: &CostTable,
    error_descriptions: &ErrorMapping,
    state: &OnDiskStateView,
    package: &CompiledPackage,
    script_path: &Path,
    script_name_opt: &Option<String>,
    signers: &[String],
    txn_args: &[TransactionArgument],
    vm_type_tags: Vec<TypeTag>,
    gas_budget: Option<u64>,
    dry_run: bool,
    verbose: bool,
) -> Result<()> {
    if !try_exists(script_path)? {
        bail!("Script file {:?} does not exist", script_path)
    };
    let bytecode_version = get_bytecode_version_from_env();

    let bytecode = if is_bytecode_file(script_path) {
        assert!(
            state.is_module_path(script_path) || !contains_module(script_path),
            "Attempting to run module {:?} outside of the `storage/` directory.
move run` must be applied to a module inside `storage/`",
            script_path
        );
        // script bytecode; read directly from file
        fs::read(script_path)?
    } else {
        // TODO(tzakian): support calling scripts in transitive deps
        let file_contents = std::fs::read_to_string(script_path)?;
        let script_opt = package
            .scripts()
            .find(|unit| unit.unit.source_map().check(&file_contents));
        // script source file; package is already compiled so load it up
        match script_opt {
            Some(unit) => unit.unit.serialize(bytecode_version),
            None => bail!("Unable to find script in file {:?}", script_path),
        }
    };

    let signer_addresses = signers
        .iter()
        .map(|s| AccountAddress::from_hex_literal(s))
        .collect::<Result<Vec<AccountAddress>, _>>()?;
    // TODO: parse Value's directly instead of going through the indirection of TransactionArgument?
    let vm_args: Vec<Vec<u8>> = convert_txn_args(txn_args);

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
    let res = match script_name_opt {
        Some(script_name) => {
            // script fun. parse module, extract script ID to pass to VM
            let module = CompiledModule::deserialize_with_defaults(&bytecode)
                .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
            session.execute_entry_function(
                &module.self_id(),
                IdentStr::new(script_name)?,
                script_type_arguments,
                vm_args,
                &mut gas_status,
            )
        }
        None => session.execute_script(
            bytecode.to_vec(),
            script_type_arguments,
            vm_args,
            &mut gas_status,
        ),
    };

    if let Err(err) = res {
        explain_execution_error(
            error_descriptions,
            err,
            state,
            &script_type_parameters,
            &script_parameters,
            &vm_type_tags,
            &signer_addresses,
            txn_args,
        )
    } else {
        let (changeset, events) = session.finish().0.map_err(|e| e.into_vm_status())?;
        if verbose {
            explain_execution_effects(&changeset, &events, state)?
        }
        maybe_commit_effects(!dry_run, changeset, events, state)
    }
}
