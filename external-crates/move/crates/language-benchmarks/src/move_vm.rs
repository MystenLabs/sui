// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use criterion::{Criterion, measurement::Measurement};
use move_binary_format::CompiledModule;
use move_compiler::{
    Compiler, command_line::compiler::PreCompiledProgramInfo, editions::Edition,
    shared::PackagePaths,
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{CORE_CODE_ADDRESS, ModuleId},
};

use move_vm_config::runtime::VMConfig;
use move_vm_runtime::{
    dev_utils::{
        in_memory_test_adapter::InMemoryTestAdapter, storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    natives::move_stdlib::stdlib_native_functions,
};
use move_vm_runtime::{
    runtime::MoveRuntime,
    shared::{gas::UnmeteredGasMeter, system_packages::SystemPackages},
};
use std::{
    path::PathBuf,
    sync::{Arc, LazyLock},
};

const BENCH_FUNCTION_PREFIX: &str = "bench_";
const BENCH_ADDR: AccountAddress = AccountAddress::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 2,
]);
const BENCH_ADDR_STR: &str = "0x2";

/// Address used by the callee package in `bench_pinned_pkg_call` benchmarks. The matching
/// literal `0x42` is hard-coded in `tests/cross_pkg_call.move`.
const LIB_ADDR: AccountAddress = AccountAddress::new([
    0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
    0x42,
]);

static PRECOMPILED_MOVE_STDLIB: LazyLock<PreCompiledProgramInfo> = LazyLock::new(|| {
    let program_res = move_compiler::construct_pre_compiled_lib(
        vec![PackagePaths {
            name: None,
            paths: move_stdlib::source_files(),
            named_address_map: move_stdlib::named_addresses(),
        }],
        None,
        None,
        false,
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
    let mut adapter = create_vm();
    publish_stdlib(&mut adapter);
    execute(c, &mut adapter, BENCH_ADDR, modules, filename);
}

/// Bench entry point for the pinned/system-package optimization (MystenLabs/sui#26508). The
/// source file is expected to contain a callee package at `LIB_ADDR` (0x42) and a user package
/// at `BENCH_ADDR` (0x2) with `bench_*` functions that hammer cross-package calls.
///
/// When `pinned` is true the callee is handed to the runtime as a `SystemPackages` entry, so
/// the JIT translator rewrites cross-package calls into direct function pointers. When false
/// the callee is published as a regular package and those calls remain virtual.
pub fn bench_pinned_pkg_call<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    filename: &str,
    pinned: bool,
) {
    let all_modules = compile_modules(filename);
    let (lib_modules, user_modules): (Vec<CompiledModule>, Vec<CompiledModule>) = all_modules
        .into_iter()
        .partition(|m| *m.self_id().address() == LIB_ADDR);
    assert!(
        !lib_modules.is_empty(),
        "{filename}: expected at least one module at {LIB_ADDR}"
    );
    assert!(
        !user_modules.is_empty(),
        "{filename}: expected at least one module at {BENCH_ADDR}"
    );

    let lib_pkg = StoredPackage::from_modules_for_testing(LIB_ADDR, lib_modules).unwrap();
    let natives = stdlib_native_functions(
        AccountAddress::from_hex_literal("0x1").unwrap(),
        move_vm_runtime::natives::move_stdlib::GasParameters::zeros(),
        /* silent debug */ true,
    )
    .unwrap();
    let runtime = if pinned {
        MoveRuntime::new_with_system_packages(
            natives,
            VMConfig::new_for_test(/* allow_unpublishable_code_execution */ false, None),
            SystemPackages::new(vec![lib_pkg.clone().into_serialized_package()]),
        )
    } else {
        MoveRuntime::new_with_test_config(natives)
    };

    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
    publish_stdlib(&mut adapter);
    // Lib bytes must live in storage either way: in the unpinned case for the user package's
    // linkage to resolve at publish time; in the pinned case for `generate_linkage_context` to
    // walk the dep graph (the pinned Arc in the runtime cache is separate from storage).
    adapter.insert_package_into_storage(lib_pkg);

    let label = if pinned {
        "system-pinned"
    } else {
        "regular-pkg"
    };
    execute_labeled(c, &mut adapter, BENCH_ADDR, user_modules, filename, label);
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
    let mut named_addresses = move_stdlib::named_addresses();
    named_addresses.insert(
        "bench".to_string(),
        move_core_types::parsing::address::NumericalAddress::parse_str(BENCH_ADDR_STR).unwrap(),
    );
    let (_files, compiled_units) = Compiler::from_files(None, src_files, vec![], named_addresses)
        .set_pre_compiled_program_opt(Some(Arc::new(PRECOMPILED_MOVE_STDLIB.clone())))
        .set_default_config(pkg_config)
        .build_and_report()
        .expect("Error compiling...");
    compiled_units
        .into_iter()
        .map(|annot_unit| annot_unit.named_module.module)
        .collect()
}

fn create_vm() -> InMemoryTestAdapter {
    InMemoryTestAdapter::new_with_runtime(MoveRuntime::new_with_test_config(
        stdlib_native_functions(
            AccountAddress::from_hex_literal("0x1").unwrap(),
            move_vm_runtime::natives::move_stdlib::GasParameters::zeros(),
            /* silent debug */ true,
        )
        .unwrap(),
    ))
}

fn publish_stdlib(adapter: &mut InMemoryTestAdapter) {
    let stdlib_modules: Vec<CompiledModule> = PRECOMPILED_MOVE_STDLIB
        .iter()
        .filter_map(|(_, info)| {
            info.compiled_unit
                .as_ref()
                .map(|unit| unit.named_module.module.clone())
        })
        .collect();
    if stdlib_modules.is_empty() {
        return;
    }
    let pkg = StoredPackage::from_modules_for_testing(CORE_CODE_ADDRESS, stdlib_modules).unwrap();
    adapter.insert_package_into_storage(pkg);
}

// TODO: cross_module benchmark is broken — uses multi-address packages not supported by current setup
// fn build_package(path: PathBuf) -> anyhow::Result<move_package::compilation::compiled_package::CompiledPackage> {
//     let config = move_package::BuildConfig {
//         dev_mode: true,
//         test_mode: false,
//         generate_docs: false,
//         install_dir: Some(path.clone()),
//         force_recompilation: false,
//         ..Default::default()
//     };
//     config.compile_package(&path, &mut Vec::new())
// }
//
// pub fn run_cross_module_tests<M: Measurement + 'static>(c: &mut Criterion<M>, path: PathBuf) {
//     let modules_a1 = build_package(path).unwrap();
//     let modules = modules_a1
//         .all_modules()
//         .map(|m| m.unit.module.clone())
//         .collect::<Vec<_>>();
//     let mut adapter = create_vm();
//     publish_stdlib(&mut adapter);
//     execute(c, &mut adapter, CORE_CODE_ADDRESS, modules, "cross_module/a1/sources/m.move");
// }

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

// execute a given function in the Bench module
fn execute<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    adapter: &mut InMemoryTestAdapter,
    sender: AccountAddress,
    modules: Vec<CompiledModule>,
    file: &str,
) {
    execute_inner(c, adapter, sender, modules, file, None);
}

// Same as `execute`, but appends `[label]` to each criterion bench name so that runs varying
// on a single dimension (e.g. system-pkg pinning on/off) don't collide on criterion bench names.
fn execute_labeled<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    adapter: &mut InMemoryTestAdapter,
    sender: AccountAddress,
    modules: Vec<CompiledModule>,
    file: &str,
    label: &str,
) {
    execute_inner(c, adapter, sender, modules, file, Some(label));
}

fn execute_inner<M: Measurement + 'static>(
    c: &mut Criterion<M>,
    adapter: &mut InMemoryTestAdapter,
    sender: AccountAddress,
    modules: Vec<CompiledModule>,
    file: &str,
    label: Option<&str>,
) {
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
            let bench_name = match label {
                Some(label) => format!(
                    "{}::{}::{} [{}]",
                    file,
                    module_id.name().as_str(),
                    fun_name,
                    label
                ),
                None => format!("{}::{}::{}", file, module_id.name().as_str(), fun_name),
            };
            c.bench_function(&bench_name, |b| {
                b.iter_with_large_drop(|| {
                    adapter
                        .make_vm(linkage.clone())
                        .unwrap()
                        .execute_function_bypass_visibility(
                            module_id,
                            fun_name,
                            vec![],
                            vec![],
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
