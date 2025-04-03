// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::{MoveCache, Package},
    dev_utils::{
        compilation_utils::{compile_packages, compile_packages_in_file},
        in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    natives::functions::NativeFunctions,
    runtime::{data_cache::TransactionDataCache, package_resolution::resolve_packages},
    shared::{linkage_context::LinkageContext, types::VersionId},
};
use move_binary_format::errors::VMResult;
use move_core_types::{account_address::AccountAddress, resolver::ModuleResolver};
use move_vm_config::runtime::VMConfig;
use std::collections::BTreeMap;
use std::sync::Arc;

fn load_linkage_packages_into_runtime<DataSource: ModuleResolver + Send + Sync>(
    adapter: &mut impl VMTestAdapter<DataSource>,
    linkage: &LinkageContext,
) -> VMResult<BTreeMap<VersionId, Arc<Package>>> {
    let cache = adapter.runtime().cache();
    let natives = adapter.runtime().natives();
    let all_packages = linkage.all_packages()?;
    let storage = TransactionDataCache::new(adapter.storage());
    resolve_packages(&cache, &natives, &storage, linkage, all_packages)
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
    let native_functions = Arc::new(NativeFunctions::new(vec![]).unwrap());
    let config = Arc::new(VMConfig::default());
    MoveCache::new(native_functions, config)
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

    assert!(adapter.runtime().cache().package_cache().read().len() == 3);
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package3_address)
        .is_some());

    assert!(
        adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 not found"
    );
    assert!(adapter.runtime().cache().package_cache().read().len() == 2);

    assert!(
        !adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 double-evicted"
    );
    assert!(adapter.runtime().cache().package_cache().read().len() == 2);
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .is_none());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package3_address)
        .is_some());

    // This should re-load package 2, but not packages 1 or 3.
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result.is_ok());
    assert!(adapter.runtime().cache().package_cache().read().len() == 3);
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package3_address)
        .is_some());

    // Re-evict 2.
    assert!(
        adapter.runtime().cache().remove_package(&package2_address),
        "Package 2 not found"
    );
    assert!(adapter.runtime().cache().package_cache().read().len() == 2);
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package1_address)
        .is_some());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package2_address)
        .is_none());
    assert!(adapter
        .runtime()
        .cache()
        .cached_package_at(package3_address)
        .is_some());
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

// Test that we properly publish and relink (and reuse) packages.
// FIXME FIXME FIXME
#[test]
fn relink() {
    /*
    let mut adapter = InMemoryTestAdapter::new();

    let st_c_v1_addr = AccountAddress::from_hex_literal("0x42").unwrap();
    let st_b_v1_addr = AccountAddress::from_hex_literal("0x43").unwrap();

    let c_runtime_addr = AccountAddress::from_hex_literal("0x2").unwrap();
    let b_runtime_addr = AccountAddress::from_hex_literal("0x3").unwrap();
    let _a_runtime_addr = AccountAddress::from_hex_literal("0x4").unwrap();

    // publish c v0
    let packages = compile_modules_in_file("rt_c_v0.move", &[]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            BTreeSet::new(),
        )
        .unwrap();

    assert_eq!(adapter.runtime()cache().package_cache().read().len(), 0);

    // publish c v1
    let packages = compile_modules_in_file("rt_c_v1.move", &[]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            st_c_v1_addr,
            modules,
            [(
                (
                    ModuleId::new(runtime_package_id, Identifier::new("c").unwrap()),
                    Identifier::new("S").unwrap(),
                ),
                ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
            )]
            .into_iter()
            .collect(),
            BTreeSet::new(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 1);

    // publish b_v0 <- c_v0
    let packages = compile_modules_in_file("rt_b_v0.move", &["rt_c_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            [c_runtime_addr].into_iter().collect(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 2);

    // publish b_v0 <- c_v1
    let packages = compile_modules_in_file("rt_b_v0.move", &["rt_c_v1.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            st_b_v1_addr,
            modules,
            [
                (
                    (
                        ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
                        Identifier::new("S").unwrap(),
                    ),
                    ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
                ),
                (
                    (
                        ModuleId::new(st_c_v1_addr, Identifier::new("c").unwrap()),
                        Identifier::new("R").unwrap(),
                    ),
                    ModuleId::new(st_c_v1_addr, Identifier::new("c").unwrap()),
                ),
            ]
            .into_iter()
            .collect(),
            [st_c_v1_addr].into_iter().collect(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 4);

    // publish a_v0 <- c_v1 && b_v0
    let packages = compile_modules_in_file("rt_a_v0.move", &["rt_c_v1.move", "rt_b_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            runtime_package_id,
            modules,
            [
                (
                    (
                        ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
                        Identifier::new("S").unwrap(),
                    ),
                    ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
                ),
                (
                    (
                        ModuleId::new(st_c_v1_addr, Identifier::new("c").unwrap()),
                        Identifier::new("R").unwrap(),
                    ),
                    ModuleId::new(st_c_v1_addr, Identifier::new("c").unwrap()),
                ),
            ]
            .into_iter()
            .collect(),
            [st_c_v1_addr, b_runtime_addr].into_iter().collect(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 5);

    // publish a_v0 <- c_v0 && b_v0 -- ERROR since a_v0 requires c_v1+
    let packages = compile_modules_in_file("rt_a_v0.move", &["rt_c_v1.move", "rt_b_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package_to_storage(
            runtime_package_id,
            runtime_package_id,
            modules,
            [(
                (
                    ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
                    Identifier::new("S").unwrap(),
                ),
                ModuleId::new(c_runtime_addr, Identifier::new("c").unwrap()),
            )]
            .into_iter()
            .collect(),
            [c_runtime_addr, b_runtime_addr].into_iter().collect(),
        )
        .unwrap_err();

    // cache stays the same since the publish failed
    assert_eq!(adapter.cache.package_cache().read().len(), 5);
    */
}
