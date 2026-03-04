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
    account_address::AccountAddress, identifier::IdentStr, language_storage::TypeTag,
    runtime_value::MoveValue,
};
use move_package_alt_compilation::compiled_package::CompiledPackage;
use move_trace_format::format::{MoveTraceBuilder, TRACE_FILE_EXTENSION};
use move_unit_test::{TRACE_DIR, vm_test_setup::VMTestSetup};
use move_vm_runtime::move_vm::MoveVM;
use sha3::{Digest, Sha3_256};
use std::{fs, path::Path};

pub fn run<V: VMTestSetup>(
    vm_test_setup: V,
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

    let vm = MoveVM::new(vm_test_setup.native_function_table()).unwrap();
    let mut gas_status = vm_test_setup.new_meter(gas_budget);
    let mut session = vm.new_session(state);

    let script_type_parameters = vec![];
    let script_parameters = vec![];

    let script_type_arguments = vm_type_tags
        .iter()
        .map(|tag| session.load_type(tag))
        .collect::<Result<Vec<_>, _>>()?;

    // TODO rethink move-cli arguments for executing functions
    let vm_args: Vec<Vec<u8>> = signer_addresses
        .iter()
        .map(|a| {
            MoveValue::Signer(*a)
                .simple_serialize()
                .expect("transaction arguments must serialize")
        })
        .chain(vm_args)
        .collect();
    let args_hash = {
        let mut hasher = Sha3_256::new();
        for arg in &vm_args {
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
    let module = CompiledModule::deserialize_with_defaults(&bytecode)
        .map_err(|e| anyhow!("Error deserializing module: {:?}", e))?;
    let module_id = module.self_id();

    let res = session.execute_function_bypass_visibility(
        &module_id,
        IdentStr::new(function_name)?,
        script_type_arguments,
        vm_args,
        &mut gas_status,
        tracer,
    );

    if trace {
        let trace_file_name = format!(
            "{}__{}__{}_{}.{}",
            module_id.address().short_str_lossless(),
            module_id.name(),
            function_name,
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
            &signer_addresses,
            txn_args,
        )
    } else {
        let _changeset = session.finish().0?;
        Ok(())
    }
}
