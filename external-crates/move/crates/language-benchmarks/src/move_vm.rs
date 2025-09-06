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

use move_vm_runtime::{
    dev_utils::{
        in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    natives::move_stdlib::stdlib_native_functions,
};
use move_vm_runtime::{runtime::MoveRuntime, shared::gas::UnmeteredGasMeter};
use once_cell::sync::Lazy;
use std::{path::PathBuf, sync::Arc};

use anyhow::Result;
use move_package::compilation::compiled_package::CompiledPackage;
use move_package::BuildConfig;

const BENCH_FUNCTION_PREFIX: &str = "bench_";

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
    let mut move_vm = create_vm();
    execute(c, &mut move_vm, modules, filename);
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

fn create_vm() -> InMemoryTestAdapter {
    InMemoryTestAdapter::new_with_runtime(MoveRuntime::new_with_default_config(
        stdlib_native_functions(
            AccountAddress::from_hex_literal("0x1").unwrap(),
            move_vm_runtime::natives::move_stdlib::GasParameters::zeros(),
            /* silent debug */ true,
        )
        .unwrap(),
    ))
}

fn find_bench_functions(modules: &[CompiledModule]) -> Vec<(Identifier, ModuleId)> {
    modules
        .iter()
        .flat_map(|module| {
            module.function_defs().iter().filter_map(|def| {
                let handle = module.function_handle_at(def.function);
                let fn_name = module.identifier_at(handle.name);
                if fn_name.as_str().starts_with(BENCH_FUNCTION_PREFIX) {
                    Some((
                        module.identifier_at(handle.name).to_owned(),
                        module.self_id(),
                    ))
                } else {
                    None
                }
            })
        })
        .collect()
}

fn build_package(dir: &str) -> Result<CompiledPackage> {
    let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    path.extend(["tests", "packages", dir]);
    let config = BuildConfig {
        dev_mode: true,
        test_mode: false,
        generate_docs: false,
        install_dir: Some(path.clone()),
        force_recompilation: false,
        ..Default::default()
    };

    config.compile_package(&path, &mut Vec::new())
    // BuildConfig::new_for_testing().build(&path).unwrap()
}

fn run_cross_module_tests() {
    let modules_a1 = build_package("a1").unwrap();
    let modules = modules_a1
        .all_modules()
        .map(|m| m.unit.module.clone())
        .collect::<Vec<_>>();
    let mut move_vm = create_vm();
    execute::<criterion::measurement::WallTime>(
        &mut Criterion::default(),
        &mut move_vm,
        modules,
        "cross_module/ModuleA.move",
    );
}

// execute a given function in the Bench module
fn execute<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    adapter: &mut InMemoryTestAdapter,
    modules: Vec<CompiledModule>,
    file: &str,
) {
    // establish running context
    let sender = CORE_CODE_ADDRESS;
    let fun_names_with_moduleid = find_bench_functions(&modules);

    let linkage = adapter
        .generate_linkage_context(sender, sender, &modules)
        .unwrap();
    let pkg = StoredPackage::from_module_for_testing_with_linkage(sender, linkage.clone(), modules)
        .unwrap();
    adapter
        .publish_package(sender, pkg.into_serialized_package())
        .unwrap();

    fun_names_with_moduleid
        .iter()
        .for_each(|(fun_name, module_id)| {
            // benchmark
            // TODO: we may want to use a real gas meter to make benchmarks more realistic.
            let bench_name = format!("{}::{}::{}", file, module_id.name().as_str(), fun_name);
            c.bench_function(&bench_name, |b| {
                b.iter_with_large_drop(|| {
                    adapter
                        .make_vm(linkage.clone())
                        .unwrap()
                        .execute_function_bypass_visibility(
                            module_id,
                            fun_name,
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
        });
}
