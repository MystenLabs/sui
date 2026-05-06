// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//! Tests for pinned system packages: install pipeline, defining-ID/identity-link checks, and
//! direct-call rewriting at JIT time.

use crate::{
    cache::move_cache::{Package, ResolvedPackageResult},
    dev_utils::{
        compilation_utils::compile_packages_in_file, in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage, vm_test_adapter::VMTestAdapter,
    },
    jit::execution::ast::{Bytecode, CallType},
    natives::functions::NativeFunctions,
    runtime::MoveRuntime,
    shared::{
        system_packages::SystemPackages,
        types::{OriginalId, VersionId},
    },
};

use move_core_types::{account_address::AccountAddress, resolver::SerializedPackage};
use move_vm_config::runtime::VMConfig;

use std::sync::Arc;

const SYS_ADDR: AccountAddress = AccountAddress::ONE;
const USER_ADDR: AccountAddress = AccountAddress::TWO;
const SYS_ADDR_TWO: AccountAddress = AccountAddress::from_suffix(3);

fn pkg_at(addr: AccountAddress, pkgs: &[StoredPackage]) -> StoredPackage {
    pkgs.iter()
        .find(|p| p.0.original_id == addr)
        .unwrap_or_else(|| panic!("no package at {addr}"))
        .clone()
}

// Counts direct vs virtual call instructions. Generic calls (`Bytecode::CallGeneric`) are
// counted by inspecting their inner `FunctionInstantiation::handle`, which carries the same
// `Direct`/`Virtual` distinction that plain calls do.
fn count_calls(pkg: &Arc<Package>) -> (usize, usize) {
    let mut direct = 0;
    let mut virtual_ = 0;
    for module in pkg.runtime.loaded_modules.values() {
        for function in module.functions.iter() {
            for op in function.code.iter() {
                match op {
                    Bytecode::DirectCall(_) => direct += 1,
                    Bytecode::VirtualCall(_) => virtual_ += 1,
                    Bytecode::CallGeneric(inst) => match &inst.handle {
                        CallType::Direct(_) => direct += 1,
                        CallType::Virtual(_) => virtual_ += 1,
                    },
                    _ => {}
                }
            }
        }
    }
    (direct, virtual_)
}

fn fresh_runtime(system: Vec<SerializedPackage>) -> MoveRuntime {
    let natives = NativeFunctions::empty_for_testing().unwrap();
    MoveRuntime::new_with_system_packages(natives, VMConfig::default(), SystemPackages::new(system))
}

// Publish a user pkg through the adapter, then force-load it via `resolve_and_cache_package`
// so the JIT'd output ends up in the cache where these tests can inspect it.
fn publish_and_cache_user_pkg(
    adapter: &mut InMemoryTestAdapter,
    original_id: OriginalId,
    pkg: SerializedPackage,
) -> Arc<Package> {
    let version_id: VersionId = pkg.version_id;
    adapter.publish_package(original_id, pkg).unwrap();
    let storage = adapter.storage().clone();
    match adapter
        .runtime()
        .resolve_and_cache_package(&storage, version_id)
        .unwrap()
    {
        ResolvedPackageResult::Found(p) => p,
        ResolvedPackageResult::NotFound => panic!("user pkg should be resolvable after publish"),
    }
}

// 1. With an empty system-package list, JIT'd user packages still produce VirtualCall for
//    cross-package calls (no behavior change vs. the existing flow).
#[test]
fn no_system_packages_uses_virtual_calls() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let user_pkg = pkg_at(USER_ADDR, &pkgs);
    let sys_pkg = pkg_at(SYS_ADDR, &pkgs);

    let runtime = fresh_runtime(vec![]);
    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
    adapter.insert_package_into_storage(sys_pkg);
    let cached =
        publish_and_cache_user_pkg(&mut adapter, USER_ADDR, user_pkg.into_serialized_package());

    let (direct, virt) = count_calls(&cached);
    assert_eq!(
        direct, 0,
        "expected no direct calls when no system pkgs are installed"
    );
    assert!(virt >= 2, "expected the two cross-pkg calls to be virtual");
}

// 2. With the system pkg installed, the user pkg's calls into it become DirectCall.
#[test]
fn system_package_user_calls_become_direct() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let sys_pkg = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    let user_pkg = pkg_at(USER_ADDR, &pkgs);

    let runtime = fresh_runtime(vec![sys_pkg]);
    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
    let cached =
        publish_and_cache_user_pkg(&mut adapter, USER_ADDR, user_pkg.into_serialized_package());

    let (direct, virt) = count_calls(&cached);
    assert!(
        direct >= 2,
        "expected the two cross-pkg calls to be direct, got {direct}"
    );
    assert_eq!(virt, 0, "expected no virtual calls remaining, got {virt}");
}

// 3. Cross-system direct calls: install pinned_a, then pinned_b which calls pinned_a; the
//    second pkg's bytecode must direct-call into the first.
#[test]
fn cross_system_calls_become_direct() {
    let pkgs = compile_packages_in_file("system_packages_chain.move", &[]);
    let a = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    let b = pkg_at(SYS_ADDR_TWO, &pkgs).into_serialized_package();

    let runtime = fresh_runtime(vec![a, b]);
    // After install the cache should hold both system packages, with `b` direct-calling `a`.
    let cache = runtime.cache();
    let pkg_b = cache
        .cached_package_at(SYS_ADDR_TWO)
        .expect("pinned_b cached");
    let (direct, virt) = count_calls(&pkg_b);
    assert!(
        direct >= 1,
        "expected pinned_b -> pinned_a to be a direct call"
    );
    assert_eq!(virt, 0, "expected no virtual calls in pinned_b");
}

// 4. Identity-link check rejects input where original_id != version_id.
#[test]
fn identity_link_check_rejects_non_identity_input() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let mut sys = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    sys.version_id = AccountAddress::from_hex_literal("0x99").unwrap();

    let runtime = fresh_runtime(vec![sys]);
    assert_eq!(runtime.cache().system_packages().len(), 0);
}

// 5. Defining-ID check rejects a system pkg whose type_origin_table points elsewhere.
#[test]
fn defining_id_check_rejects_mismatched_origin() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let mut sys = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    // Re-stamp every type origin to a different defining ID. The pkg has no types in this file,
    // so inject one to force the check.
    sys.type_origin_table.insert(
        move_core_types::resolver::IntraPackageName {
            module_name: move_core_types::identifier::Identifier::new("sys").unwrap(),
            type_name: move_core_types::identifier::Identifier::new("FakeType").unwrap(),
        },
        AccountAddress::from_hex_literal("0x99").unwrap(),
    );

    let runtime = fresh_runtime(vec![sys]);
    assert_eq!(
        runtime.cache().system_packages().len(),
        0,
        "system pkg with mismatched defining_id should be rejected",
    );
}

// 6. Validation failure logs and skips, runtime still constructs and is usable.
#[test]
fn validation_failure_logs_and_continues() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let mut sys = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    // Corrupt one of the serialized modules.
    if let Some((_, bytes)) = sys.modules.iter_mut().next() {
        bytes.clear();
    }

    let runtime = fresh_runtime(vec![sys]);
    assert_eq!(runtime.cache().system_packages().len(), 0);

    // The runtime still works for non-system flows: publishing a stand-alone pkg succeeds.
    let stand_alone = compile_packages_in_file("system_packages_basic.move", &[]);
    let user = pkg_at(USER_ADDR, &stand_alone);
    let lib = pkg_at(SYS_ADDR, &stand_alone);
    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
    adapter.insert_package_into_storage(lib);
    adapter
        .publish_package(USER_ADDR, user.into_serialized_package())
        .unwrap();
}

// 7. A user pkg whose linkage maps the system OriginalId to a *different* version ID must
//    NOT direct-call into our pinned system pkg. This guards against a host that maps
//    `0x1 -> some_other_version` accidentally pulling our global into their dispatch.
#[test]
fn linkage_with_wrong_version_does_not_direct_resolve() {
    use crate::shared::linkage_context::LinkageContext;

    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let sys = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    let runtime = fresh_runtime(vec![sys.clone()]);

    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);

    // Re-publish the "user" pkg with a hand-crafted linkage that maps `0x1 -> 0x42` (a wholly
    // different version). The system pkg is still in our runtime as pinned `0x1 -> 0x1`, but
    // the user pkg's linkage points at a different version, so direct-resolution must skip.
    let user_modules: Vec<_> = pkg_at(USER_ADDR, &pkgs)
        .0
        .modules
        .values()
        .map(|bs| move_binary_format::CompiledModule::deserialize_with_defaults(bs).unwrap())
        .collect();

    let mut linkage = std::collections::BTreeMap::new();
    linkage.insert(USER_ADDR, USER_ADDR);
    linkage.insert(SYS_ADDR, AccountAddress::from_hex_literal("0x42").unwrap());

    // Park the imposter `0x1 -> 0x42` package in storage so validation can find it. We use the
    // real system bytes but at version_id 0x42.
    let mut imposter = sys.clone();
    imposter.version_id = AccountAddress::from_hex_literal("0x42").unwrap();
    imposter.linkage_table = std::collections::BTreeMap::from([(SYS_ADDR, imposter.version_id)]);
    adapter.insert_package_into_storage(StoredPackage(imposter));

    let stored = StoredPackage::from_module_for_testing_with_linkage(
        USER_ADDR,
        LinkageContext::new(linkage).unwrap(),
        user_modules,
    )
    .unwrap();

    let cached =
        publish_and_cache_user_pkg(&mut adapter, USER_ADDR, stored.into_serialized_package());
    let (direct, virt) = count_calls(&cached);
    assert_eq!(
        direct, 0,
        "user pkg whose linkage points at a different version of `0x1` must not direct-call our pinned `0x1`",
    );
    assert!(
        virt >= 2,
        "calls remain virtual under a non-matching linkage"
    );
}

// 8. Successfully installed system packages are findable in the regular package cache too,
//    so the standard `resolve_packages` path picks them up without a storage round-trip.
#[test]
fn system_packages_are_in_regular_cache() {
    let pkgs = compile_packages_in_file("system_packages_basic.move", &[]);
    let sys = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    let runtime = fresh_runtime(vec![sys]);

    let cached: Option<Arc<Package>> = runtime.cache().cached_package_at(SYS_ADDR);
    assert!(
        cached.is_some(),
        "system pkg should be in the regular package_cache by VersionId"
    );
}

// 9. The retained system-package map carries the right ID after install.
#[test]
fn system_packages_map_records_original_id() {
    let pkgs = compile_packages_in_file("system_packages_chain.move", &[]);
    let a = pkg_at(SYS_ADDR, &pkgs).into_serialized_package();
    let b = pkg_at(SYS_ADDR_TWO, &pkgs).into_serialized_package();

    let runtime = fresh_runtime(vec![a, b]);
    let cache = runtime.cache();
    let sys = cache.system_packages();
    assert_eq!(sys.len(), 2);
    assert!(sys.contains_key(&OriginalId::from(SYS_ADDR)));
    assert!(sys.contains_key(&OriginalId::from(SYS_ADDR_TWO)));
}

// 10. End-to-end with the real move-stdlib: install stdlib at 0x1 and JIT a user pkg that
//     calls into it; user calls into stdlib must be direct.
#[test]
fn move_stdlib_installs_and_user_calls_become_direct() {
    use move_compiler::{
        Compiler as MoveCompiler,
        compiled_unit::AnnotatedCompiledUnit,
        diagnostics::warning_filters::WarningFiltersBuilder,
        editions::{Edition, Flavor},
        shared::PackageConfig,
    };
    use std::{fs::File, io::Write};
    use tempfile::tempdir;

    // Compile move-stdlib from source.
    let (_, stdlib_units) = MoveCompiler::from_files(
        None,
        move_stdlib::source_files(),
        vec![],
        move_stdlib::named_addresses(),
    )
    .set_default_config(PackageConfig {
        is_dependency: false,
        warning_filter: WarningFiltersBuilder::unused_warnings_filter_for_test(),
        flavor: Flavor::Core,
        edition: Edition::E2024_ALPHA,
    })
    .build_and_report()
    .expect("stdlib compilation");
    let stdlib_modules: Vec<_> = stdlib_units
        .into_iter()
        .map(|u: AnnotatedCompiledUnit| u.named_module.module)
        .collect();
    let stdlib_pkg = StoredPackage::from_modules_for_testing(SYS_ADDR, stdlib_modules)
        .expect("stdlib stored pkg");
    let stdlib_serialized = stdlib_pkg.clone().into_serialized_package();

    // Compile a small user module that calls into stdlib.
    let dir = tempdir().unwrap();
    let user_src = dir.path().join("user.move");
    let mut f = File::create(&user_src).unwrap();
    // Call a stdlib function that is NOT a vector primitive (the compiler folds
    // `vector::empty/length/etc.` into specialized opcodes that bypass the call machinery, so
    // those wouldn't exercise the direct-call path).
    writeln!(
        f,
        "module 0x2::caller {{
    public fun encode(x: u64): vector<u8> {{ std::bcs::to_bytes(&x) }}
}}",
    )
    .unwrap();

    let (_, user_units) = MoveCompiler::from_files(
        None,
        vec![user_src.to_string_lossy().to_string()],
        move_stdlib::source_files(),
        move_stdlib::named_addresses(),
    )
    .set_default_config(PackageConfig {
        is_dependency: false,
        warning_filter: WarningFiltersBuilder::unused_warnings_filter_for_test(),
        flavor: Flavor::Core,
        edition: Edition::E2024_ALPHA,
    })
    .build_and_report()
    .expect("user compilation");
    let user_modules: Vec<_> = user_units
        .into_iter()
        .filter_map(|u| {
            let m = u.named_module.module;
            (*m.self_id().address() == USER_ADDR).then_some(m)
        })
        .collect();
    let user_pkg =
        StoredPackage::from_modules_for_testing(USER_ADDR, user_modules).expect("user stored pkg");

    // Build a runtime whose natives table actually has the stdlib natives wired up;
    // `NativeFunctions::empty_for_testing()` would cause MISSING_DEPENDENCY on `bcs`, etc.
    let stdlib_natives = crate::natives::move_stdlib::stdlib_native_functions(
        SYS_ADDR,
        crate::natives::move_stdlib::GasParameters::zeros(),
        /* debug_is_silent */ true,
    )
    .expect("stdlib natives table");
    let runtime = MoveRuntime::new_with_system_packages(
        stdlib_natives,
        VMConfig::default(),
        SystemPackages::new(vec![stdlib_serialized]),
    );
    assert_eq!(
        runtime.cache().system_packages().len(),
        1,
        "stdlib should install cleanly as a system pkg",
    );
    let mut adapter = InMemoryTestAdapter::new_with_runtime(runtime);
    let cached =
        publish_and_cache_user_pkg(&mut adapter, USER_ADDR, user_pkg.into_serialized_package());

    // The single call into stdlib (`bcs::to_bytes`) should be direct.
    let (direct, virt) = count_calls(&cached);
    assert!(
        direct >= 1,
        "user pkg's stdlib call should be direct, got direct={direct} virt={virt}",
    );
    assert_eq!(virt, 0, "no remaining virtual calls expected");
}
