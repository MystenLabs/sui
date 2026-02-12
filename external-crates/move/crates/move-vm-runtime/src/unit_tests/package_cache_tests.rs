// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::{MoveCache, Package},
    dev_utils::{
        compilation_utils::{
            compile_packages, compile_packages_in_file, expect_modules, make_base_path,
        },
        in_memory_test_adapter::InMemoryTestAdapter,
        storage::StoredPackage,
        vm_test_adapter::VMTestAdapter,
    },
    runtime::{package_resolution::resolve_packages, telemetry::TransactionTelemetryContext},
    shared::{
        linkage_context::LinkageContext,
        types::{OriginalId, VersionId},
    },
};
use indexmap::IndexMap;
use move_binary_format::{CompiledModule, errors::VMResult};
use move_compiler::Compiler;
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    resolver::{IntraPackageName, ModuleResolver},
};
use move_vm_config::runtime::VMConfig;
use std::collections::BTreeMap;
use std::sync::Arc;

fn load_linkage_packages_into_runtime<DataSource: ModuleResolver + Send + Sync>(
    adapter: &mut impl VMTestAdapter<DataSource>,
    linkage: &LinkageContext,
) -> VMResult<BTreeMap<VersionId, Arc<Package>>> {
    let mut dummy_telemetry = TransactionTelemetryContext::new();
    let cache = adapter.runtime().cache();
    let natives = adapter.runtime().natives();
    let all_packages = linkage.all_packages()?;
    resolve_packages(
        adapter.storage(),
        &mut dummy_telemetry,
        &cache,
        &natives,
        all_packages,
    )
}

#[test]
fn cache_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package1.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 3);
    assert_eq!(l_pkg.runtime.version_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn cache_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package2.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 3);
    assert_eq!(l_pkg.runtime.version_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn cache_package_external_package_calls_no_types() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package3.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the packages correctly
    let results = result.unwrap();
    let l_pkg = results.get(&package1_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 2);
    assert_eq!(l_pkg.runtime.version_id, package1_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 2);

    let l_pkg = results.get(&package2_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 1);
    assert_eq!(l_pkg.runtime.version_id, package2_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 1);
}

/// Generate a new, dummy cachce for testing.
#[allow(dead_code)]
fn dummy_cache_for_testing() -> MoveCache {
    let config = Arc::new(VMConfig::default());
    MoveCache::new(config)
}

#[test]
fn load_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package1.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 3);
    assert_eq!(l_pkg.runtime.version_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn load_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package1.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }
    let link_context = adapter.get_linkage_context(package_address).unwrap();

    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 3);
    assert_eq!(l_pkg.runtime.version_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
    assert_eq!(l_pkg.runtime.vtable.types.len(), 0);
}

#[test]
fn load_package_external_package_calls_no_types() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package3.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 2);
    let l_pkg = l_pkg.get(&package2_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.len(), 1);
    assert_eq!(l_pkg.runtime.version_id, package2_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 1);
}

#[test]
fn cache_package_external_package_type_references() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package4.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );
}

#[test]
fn cache_package_external_generic_call_type_references() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();

    let mut packages = compile_packages("package6.move", &[]);

    let a_pkg = packages.remove(&package1_address).unwrap();
    let b_pkg = packages.remove(&package2_address).unwrap();

    // publish a
    adapter
        .publish_package(package1_address, a_pkg.into_serialized_package())
        .unwrap();

    // publish b
    adapter
        .publish_package(
            package2_address,
            b_pkg.into_serialized_package(),
            // TODO: test with this custom linkage instead
            // [(
            //     (
            //         ModuleId::new(package1_address, Identifier::new("a").unwrap()),
            //         Identifier::new("AA").unwrap(),
            //     ),
            //     ModuleId::new(package1_address, Identifier::new("a").unwrap()),
            // )]
            // .into_iter()
            // .collect(),
            // [package1_address].into_iter().collect(),
        )
        .unwrap();
}

#[test]
fn cache_package_external_package_type_references_cache_reload() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package4.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result1.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result2.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );

    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );
}

#[test]
fn cache_package_external_package_type_references_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package5.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package3_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );
}

#[test]
fn cache_and_evict_packages() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package5.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package3_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result.is_ok());

    assert!(adapter.runtime().cache().package_cache().len() == 3);
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .is_some()
    );

    assert!(
        adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 not found"
    );
    assert!(adapter.runtime().cache().package_cache().len() == 2);

    assert!(
        !adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 double-evicted"
    );
    assert!(adapter.runtime().cache().package_cache().len() == 2);
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .is_none()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .is_some()
    );

    // This should re-load package 2, but not packages 1 or 3.
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result.is_ok());
    assert!(adapter.runtime().cache().package_cache().len() == 3);
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .is_some()
    );

    // Re-evict 2.
    assert!(
        adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 not found"
    );
    assert!(adapter.runtime().cache().package_cache().len() == 2);
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .is_some()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .is_none()
    );
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .is_some()
    );
}

#[test]
fn cache_package_external_package_type_references_cache_reload_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package5.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    // Load from the bottom up
    let link_context = adapter.get_linkage_context(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result1.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result2.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    let link_context = adapter.get_linkage_context(package3_address).unwrap();
    let result3 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result3.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    // Now load it the other way -- from the top down. We do it in a new adapter to get a new
    // cache, etc., all set up.
    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package5.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    let link_context = adapter.get_linkage_context(package3_address).unwrap();
    let result3 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result3.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    let link_context = adapter.get_linkage_context(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result1.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );

    let link_context = adapter.get_linkage_context(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result2.is_ok());
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package3_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package2_address)
            .expect("not found")
            .loaded_types_len(),
        3
    );
    assert_eq!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(package1_address)
            .expect("not found")
            .loaded_types_len(),
        4
    );
}

// Test that the package cache correctly reuses cached packages when different linkage
// contexts share common dependencies. Uses Arc::ptr_eq to verify that the same cached
// Arc<Package> is returned rather than a recompiled copy.
//
// package5.move defines:
//   0x1: modules a, b (b depends on a)
//   0x2: module c (depends on 0x1::a, 0x1::b)
//   0x3: module c (depends on 0x1::a, 0x1::b, 0x2::c)
#[test]
fn cache_reuses_packages_across_linkage_contexts() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    for pkg in compile_packages_in_file("package5.move", &[]) {
        adapter.insert_package_into_storage(pkg);
    }

    // Load linkage for package 2 (pulls in packages 1 and 2)
    let link_context_2 = adapter.get_linkage_context(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context_2);
    assert!(result.is_ok());
    assert_eq!(adapter.runtime().cache().package_cache().len(), 2);

    // Grab the cached Arc for package 1 before loading the next linkage context
    let pkg1_before = adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .expect("package 1 should be cached");

    // Load linkage for package 3 (pulls in packages 1, 2, and 3).
    // Package 1 and 2 should come from the cache, only 3 is new.
    let link_context_3 = adapter.get_linkage_context(package3_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context_3);
    assert!(result.is_ok());
    assert_eq!(adapter.runtime().cache().package_cache().len(), 3);

    // Verify package 1 is the exact same Arc (pointer-equal), proving cache reuse
    let pkg1_after = adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .expect("package 1 should still be cached");
    assert!(
        Arc::ptr_eq(&pkg1_before, &pkg1_after),
        "package 1 should be the same Arc instance, not recompiled"
    );

    // Same check for package 2
    let pkg2_before = adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .expect("package 2 should be cached");

    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context_3);
    assert!(result.is_ok());

    let pkg2_after = adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .expect("package 2 should still be cached");
    assert!(
        Arc::ptr_eq(&pkg2_before, &pkg2_after),
        "package 2 should be the same Arc instance, not recompiled"
    );

    // Evict the shared dependency (package 1) and reload
    assert!(adapter.runtime().cache().remove_package(&package1_address));
    assert_eq!(adapter.runtime().cache().package_cache().len(), 2);

    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context_3);
    assert!(result.is_ok());
    assert_eq!(adapter.runtime().cache().package_cache().len(), 3);

    // After eviction and reload, package 1 should be a NEW Arc (different pointer)
    let pkg1_reloaded = adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .expect("package 1 should be re-cached");
    assert!(
        !Arc::ptr_eq(&pkg1_before, &pkg1_reloaded),
        "after eviction, package 1 should be a fresh compilation"
    );

    // But the contents should be equivalent
    assert_eq!(pkg1_before.loaded_types_len(), pkg1_reloaded.loaded_types_len());
}

/// Compile a relinker test module with the given original/version IDs and dependencies.
fn relinker_pkg(
    original_id: OriginalId,
    version_id: VersionId,
    module: &str,
    deps: &[&str],
) -> StoredPackage {
    let base = make_base_path();
    let source = base
        .join(format!("rt_{module}.move"))
        .to_string_lossy()
        .to_string();
    let dep_paths: Vec<String> = deps
        .iter()
        .map(|d| base.join(format!("rt_{d}.move")).to_string_lossy().to_string())
        .collect();

    let (_, units) = Compiler::from_files(
        None,
        vec![source],
        dep_paths,
        BTreeMap::<String, _>::new(),
    )
    .build_and_report()
    .expect("compilation failed");

    let modules: Vec<CompiledModule> = expect_modules(units)
        .filter(|m| *m.self_id().address() == original_id)
        .collect();
    StoredPackage::from_modules_for_testing(version_id, modules).unwrap()
}

// Test that publishing and relinking packages properly updates the cache:
// - Different linkage contexts sharing a dependency reuse its cached Arc
// - Different versions of the same original package (C v0 vs C v1) are cached separately
// - A failed publish (linkage mismatch) does not corrupt the cache
//
// Package dependency structure:
//   C v0 (0x2): struct S, fun c() -> 42
//   C v1 (0x5): struct S, struct R, fun c() -> 43, fun d() -> 44
//   B v0 (0x3): fun b() = c::c() + 1  (depends on C)
//   A v0 (0x4): fun a() = b::b() + c::d()  (depends on B and C v1)

//Stores 4 packages with a relinking dependency chain: C v0 (0x2), C v1 (0x5, upgrade of C), B v0 (0x3, depends on C), A v0 (0x4, depends on B + C v1)
//Loads B's dependency tree (B + C v0) into cache — verifies cache = 2
//Loads A's dependency tree (A + B + C v1) into cache — verifies cache = 4 and that B v0 is reused from cache (Arc::ptr_eq) rather than recompiled
//Verifies C v0 and C v1 are separate cache entries (different VersionIds)
//Attempts a bad publish of A linked against C v0 (A calls d() which only exists in C v1) — verifies it fails and cache is unchanged
#[test]
fn relink() {
    let c_orig = AccountAddress::from_hex_literal("0x2").unwrap();
    let b_orig = AccountAddress::from_hex_literal("0x3").unwrap();
    let a_orig = AccountAddress::from_hex_literal("0x4").unwrap();
    let c_v1_addr = AccountAddress::from_hex_literal("0x5").unwrap();

    let mut adapter = InMemoryTestAdapter::new();

    // -- Store all packages in storage --

    // C v0 (original=0x2, stored at 0x2)
    let c0 = relinker_pkg(c_orig, c_orig, "c_v0", &[]);
    adapter.insert_package_into_storage(c0);

    // C v1 (original=0x2, stored at 0x5) with type origins:
    // S was defined in C v0 (0x2), R is new in C v1 (0x5)
    let mut c1 = relinker_pkg(c_orig, c_v1_addr, "c_v1", &[]);
    c1.0.type_origin_table = IndexMap::from([
        (
            IntraPackageName {
                module_name: Identifier::new("c").unwrap(),
                type_name: Identifier::new("S").unwrap(),
            },
            c_orig,
        ),
        (
            IntraPackageName {
                module_name: Identifier::new("c").unwrap(),
                type_name: Identifier::new("R").unwrap(),
            },
            c_v1_addr,
        ),
    ]);
    adapter.insert_package_into_storage(c1);

    // B v0 (original=0x3, stored at 0x3) linked against C v0
    let mut b0 = relinker_pkg(b_orig, b_orig, "b_v0", &["c_v0"]);
    b0.0.linkage_table = BTreeMap::from([(c_orig, c_orig), (b_orig, b_orig)]);
    adapter.insert_package_into_storage(b0);

    // A v0 (original=0x4, stored at 0x4) linked against C v1 and B v0
    let mut a0 = relinker_pkg(a_orig, a_orig, "a_v0", &["b_v0", "c_v1"]);
    a0.0.linkage_table =
        BTreeMap::from([(c_orig, c_v1_addr), (b_orig, b_orig), (a_orig, a_orig)]);
    adapter.insert_package_into_storage(a0);

    // -- Load packages into cache with different linkage contexts --

    assert_eq!(adapter.runtime().cache().package_cache().len(), 0);

    // Load B v0's dependency tree: B v0 + C v0
    let link_b = LinkageContext::new(BTreeMap::from([(c_orig, c_orig), (b_orig, b_orig)]));
    load_linkage_packages_into_runtime(&mut adapter, &link_b).unwrap();
    assert_eq!(adapter.runtime().cache().package_cache().len(), 2);

    // Capture B v0's cached Arc to verify reuse across linkage contexts
    let b_cached = adapter
        .runtime()
        .cache()
        .cached_package_at(b_orig)
        .expect("B v0 should be cached");

    // Load A v0's dependency tree: A v0 + B v0 + C v1.
    // B v0 (at 0x3) should be reused from cache.
    // C v1 (at 0x5) is new — a different VersionId from C v0 (at 0x2).
    let link_a = LinkageContext::new(BTreeMap::from([
        (c_orig, c_v1_addr),
        (b_orig, b_orig),
        (a_orig, a_orig),
    ]));
    load_linkage_packages_into_runtime(&mut adapter, &link_a).unwrap();
    // Cache now has: C v0 (0x2), C v1 (0x5), B v0 (0x3), A v0 (0x4)
    assert_eq!(adapter.runtime().cache().package_cache().len(), 4);

    // B v0 is the same Arc (reused, not recompiled)
    let b_reused = adapter
        .runtime()
        .cache()
        .cached_package_at(b_orig)
        .expect("B v0 should still be cached");
    assert!(
        Arc::ptr_eq(&b_cached, &b_reused),
        "B v0 should be reused across linkage contexts"
    );

    // Both C versions are cached as separate entries
    assert!(adapter.runtime().cache().cached_package_at(c_orig).is_some());
    assert!(
        adapter
            .runtime()
            .cache()
            .cached_package_at(c_v1_addr)
            .is_some()
    );

    let cache_before_bad = adapter.runtime().cache().package_cache().len();

    // Try publishing A v0 linked against C v0 instead of C v1.
    // A calls d() which only exists in C v1, so verification should fail.
    let mut a0_bad = relinker_pkg(a_orig, a_orig, "a_v0", &["b_v0", "c_v1"]);
    a0_bad.0.linkage_table =
        BTreeMap::from([(c_orig, c_orig), (b_orig, b_orig), (a_orig, a_orig)]);
    let result = adapter.publish_package(a_orig, a0_bad.into_serialized_package());
    assert!(
        result.is_err(),
        "A linked against C v0 should fail (missing d())"
    );

    // Cache must not change from a failed publish
    assert_eq!(
        adapter.runtime().cache().package_cache().len(),
        cache_before_bad,
    );
}
