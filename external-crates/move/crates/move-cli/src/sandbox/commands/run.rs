// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::sandbox::utils::{
    contains_module, explain_execution_error, is_bytecode_file, on_disk_state_view::OnDiskStateView,
};
use anyhow::{Result, anyhow, bail};
use move_binary_format::file_format::CompiledModule;
use move_command_line_common::files::try_exists;
use move_core_types::{
    identifier::IdentStr,
    language_storage::TypeTag,
    runtime_value::{MoveValue, serialize_values},
};
use move_package_alt_compilation::compiled_package::CompiledPackage;
use move_trace_format::format::{MoveTraceBuilder, TRACE_FILE_EXTENSION};
use move_unit_test::{TRACE_DIR, vm_test_setup::VMTestSetup};
use move_vm_runtime::{
    dev_utils::vm_arguments::ValueFrame, natives::functions::NativeFunctions, runtime::MoveRuntime,
};
use sha3::{Digest, Sha3_256};
use std::{fs, path::Path};

pub fn run<V: VMTestSetup>(
    vm_test_setup: V,
    state: &OnDiskStateView,
    _package: &CompiledPackage,
    module_file: &Path,
    function: &str,
    txn_args: &[MoveValue],
    vm_type_tags: Vec<TypeTag>,
    gas_budget: Option<u64>,
    _dry_run: bool,
    _verbose: bool,
    trace: bool,
) -> Result<()> {
    move_vm_config::tracing_feature_disabled! {
        if trace {
            return Err(anyhow!(
                "Tracing is enabled but the binary was not compiled with the `tracing` \
                 feature flag set. Rebuild binary with `--features tracing`"
            ));
        }
    };
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

    let natives = NativeFunctions::new(vm_test_setup.native_function_table())?;
    let runtime = MoveRuntime::new_with_default_config(natives);

    let mut gas_status = vm_test_setup.new_meter(gas_budget);

    let script_type_parameters = vec![];
    let script_parameters = vec![];
    let module = CompiledModule::deserialize_with_defaults(&bytecode)
        .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
    let module_id = module.self_id();
    let serialized_args = serialize_values(txn_args.iter());

    let args_hash = {
        let mut hasher = Sha3_256::new();
        for arg in &serialized_args {
            hasher.update(arg);
        }
        for tag in &vm_type_tags {
            hasher.update(tag.to_string().as_bytes());
        }
        let result = hasher.finalize();
        result[..8]
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>()
    };
    let mut move_tracer = MoveTraceBuilder::new();
    let tracer = if trace { Some(&mut move_tracer) } else { None };

    let res = {
        // script fun. parse module, extract script ID to pass to VM
        let mut linkage = state.generate_linkage_context(module.address())?;
        linkage.add_type_arg_addresses_reflexive(&vm_type_tags);

        let mut vm_instance = runtime.make_vm(state, linkage)?;
        let type_args = vm_type_tags
            .iter()
            .map(|tag| vm_instance.load_type(tag))
            .collect::<Result<Vec<_>, _>>()?;
        let function = IdentStr::new(function)?;

        ValueFrame::serialized_call(
            &mut vm_instance,
            &module.self_id(),
            function,
            type_args,
            serialized_args,
            &mut gas_status,
            tracer,
            false, /*  bypass_declared_entry_check */
        )
    };

    if trace {
        let trace_file_name = format!(
            "{}__{}__{}_{}.{}",
            module_id.address().short_str_lossless(),
            module_id.name(),
            function,
            args_hash,
            TRACE_FILE_EXTENSION
        );
        let trace_dir = Path::new(TRACE_DIR);
        fs::create_dir_all(trace_dir)?;
        let trace_path = trace_dir.join(trace_file_name);
        let trace_bytes = move_tracer.into_trace().into_compressed_json_bytes();
        fs::write(&trace_path, trace_bytes)?;
    }

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
