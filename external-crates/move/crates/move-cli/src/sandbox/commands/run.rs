// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    sandbox::utils::{
        contains_module, explain_execution_error, get_gas_status, is_bytecode_file,
        on_disk_state_view::OnDiskStateView,
    },
    NativeFunctionRecord,
};
use anyhow::{anyhow, bail, Result};
use move_binary_format::file_format::CompiledModule;
use move_command_line_common::files::try_exists;
use move_core_types::{
    account_address::AccountAddress,
    errmap::ErrorMapping,
    identifier::IdentStr,
    language_storage::TypeTag,
    runtime_value::MoveValue,
    transaction_argument::{convert_txn_args, TransactionArgument},
};
use move_package::compilation::compiled_package::CompiledPackage;
#[cfg(feature = "gas-profiler")]
use move_vm_profiler::GasProfiler;
use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::gas_schedule::CostTable;
#[cfg(feature = "gas-profiler")]
use move_vm_types::gas::GasMeter;
use std::{fs, path::Path};

pub fn run(
    natives: impl IntoIterator<Item = NativeFunctionRecord>,
    cost_table: &CostTable,
    error_descriptions: &ErrorMapping,
    state: &OnDiskStateView,
    _package: &CompiledPackage,
    module_file: &Path,
    function_name: &str,
    signers: &[String],
    txn_args: &[TransactionArgument],
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
    let res = {
        // script fun. parse module, extract script ID to pass to VM
        let module = CompiledModule::deserialize_with_defaults(&bytecode)
            .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
        #[cfg(feature = "gas-profiler")]
        {
            let gas_rem: u64 = gas_status.remaining_gas().into();
            gas_status.set_profiler(GasProfiler::init(
                &session.vm_config().profiler_config,
                function_name.to_owned(),
                gas_rem,
            ));
        }

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
        let (_changeset, _events) = session.finish().0?;
        Ok(())
    }
}
