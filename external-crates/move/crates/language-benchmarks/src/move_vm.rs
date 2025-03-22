// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{measurement::Measurement, Criterion};
use move_binary_format::CompiledModule;
use move_compiler::{editions::Edition, shared::PackagePaths, Compiler, FullyCompiledProgram};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, CORE_CODE_ADDRESS},
};

use move_vm_runtime::move_vm::MoveVM;
use move_vm_test_utils::BlankStorage;
use move_vm_types::gas::UnmeteredGasMeter;
use once_cell::sync::Lazy;
use std::{path::PathBuf, sync::Arc};

static PRECOMPILED_MOVE_STDLIB: Lazy<FullyCompiledProgram> = Lazy::new(|| {
    let program_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: None,
            paths: move_stdlib::move_stdlib_files(),
            named_address_map: move_stdlib::move_stdlib_named_addresses(),
        }],
        None,
        move_compiler::Flags::empty(),
        None,
    )
    .unwrap();
    match program_res {
        Ok(stdlib) => stdlib,
        Err((files, errors)) => {
            eprintln!("!!!Standard library failed to compile!!!");
            move_compiler::diagnostics::report_diagnostics(&files, errors)
        }
    }
});

/// Entry point for the bench, provide a function name to invoke in Module Bench in bench.move.
pub fn bench<M: Measurement + 'static>(c: &mut Criterion<M>, filename: &str) {
    let modules = compile_modules(filename);
    let move_vm = create_vm();
    execute(c, &move_vm, modules, filename);
}

fn make_path(file: &str) -> PathBuf {
    vec![env!("CARGO_MANIFEST_DIR"), "tests", file]
        .into_iter()
        .collect()
}

// Compile `bench.move` and its dependencies
pub fn compile_modules(filename: &str) -> Vec<CompiledModule> {
    let src_files = vec![make_path(filename).to_str().unwrap().to_owned()];
    let pkg_config = move_compiler::shared::PackageConfig {
        edition: Edition::E2024_BETA,
        ..Default::default()
    };
    let (_files, compiled_units) = Compiler::from_files(
        None,
        src_files,
        vec![],
        move_stdlib::move_stdlib_named_addresses(),
    )
    .set_pre_compiled_lib(Arc::new(PRECOMPILED_MOVE_STDLIB.clone()))
    .set_default_config(pkg_config)
    .build_and_report()
    .expect("Error compiling...");
    compiled_units
        .into_iter()
        .map(|annot_unit| annot_unit.named_module.module)
        .collect()
}

fn create_vm() -> MoveVM {
    MoveVM::new(move_stdlib_natives::all_natives(
        AccountAddress::from_hex_literal("0x1").unwrap(),
        move_stdlib_natives::GasParameters::zeros(),
        /* silent debug */ true,
    ))
    .unwrap()
}

// execute a given function in the Bench module
fn execute<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    move_vm: &MoveVM,
    modules: Vec<CompiledModule>,
    file: &str,
) {
    // establish running context
    let storage = BlankStorage::new();
    let sender = CORE_CODE_ADDRESS;
    let mut session = move_vm.new_session(&storage);

    // TODO: we may want to use a real gas meter to make benchmarks more realistic.

    for module in modules {
        let mut mod_blob = vec![];
        module
            .serialize_with_version(module.version, &mut mod_blob)
            .expect("Module serialization error");
        session
            .publish_module(mod_blob, sender, &mut UnmeteredGasMeter)
            .expect("Module must load");
    }

    // module and function to call
    let module_id = ModuleId::new(sender, Identifier::new("bench").unwrap());
    let fun_name = Identifier::new("bench").unwrap();

    // benchmark
    c.bench_function(file, |b| {
        b.iter_with_large_drop(|| {
            session
                .execute_function_bypass_visibility(
                    &module_id,
                    &fun_name,
                    vec![],
                    Vec::<Vec<u8>>::new(),
                    &mut UnmeteredGasMeter,
                    None,
                )
                .unwrap_or_else(|err| {
                    panic!("{:?}::bench in {file} failed with {:?}", &module_id, err)
                })
        })
    });
}
