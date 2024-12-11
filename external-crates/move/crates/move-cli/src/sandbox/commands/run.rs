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
    identifier::IdentStr,
    language_storage::TypeTag,
    transaction_argument::{convert_txn_args, TransactionArgument},
};
use move_package::compilation::compiled_package::CompiledPackage;
use move_vm_runtime::{
    dev_utils::gas_schedule::CostTable, natives::functions::NativeFunctions, runtime::MoveRuntime,
};
use std::{fs, path::Path};

pub fn run(
    natives: impl IntoIterator<Item = NativeFunctionRecord>,
    cost_table: &CostTable,
    state: &OnDiskStateView,
    _package: &CompiledPackage,
    module_file: &Path,
    function: &str,
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
            && (state.is_package_path(module_file) || !contains_module(module_file)),
        "Attempting to run module {:?} outside of the `storage/` directory.
        move run` must be applied to a module inside `storage/`",
        module_file
    );
    let bytecode = fs::read(module_file)?;

    // TODO: parse Value's directly instead of going through the indirection of TransactionArgument?
    let vm_args: Vec<Vec<u8>> = convert_txn_args(txn_args);

    let natives = NativeFunctions::new(natives)?;
    let runtime = MoveRuntime::new_with_default_config(natives);

    let mut gas_status = get_gas_status(cost_table, gas_budget)?;

    let script_type_parameters = vec![];
    let script_parameters = vec![];

    // // TODO rethink move-cli arguments for executing functions
    let res = {
        // script fun. parse module, extract script ID to pass to VM
        let module = CompiledModule::deserialize_with_defaults(&bytecode)
            .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
        move_vm_profiler::tracing_feature_enabled! {
            use move_vm_profiler::GasProfiler;
            use move_vm_types::gas::GasMeter;

            let gas_rem: u64 = gas_status.remaining_gas().into();
            gas_status.set_profiler(GasProfiler::init(
                &session.vm_config().profiler_config,
                function_name.to_owned(),
                gas_rem,
            ));
        }

        let mut linkage = state.generate_linkage_context(&module.address())?;
        linkage.add_type_arg_addresses_reflexive(&vm_type_tags);

        let mut vm_instance = runtime.make_vm(state, linkage)?;
        let type_args = vm_type_tags
            .iter()
            .map(|tag| vm_instance.load_type(tag))
            .collect::<Result<Vec<_>, _>>()?;
        let function = IdentStr::new(function)?;
        vm_instance.execute_entry_function(
            &module.self_id(),
            function,
            type_args,
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
            txn_args,
        )
    } else {
        Ok(())
    }
}
