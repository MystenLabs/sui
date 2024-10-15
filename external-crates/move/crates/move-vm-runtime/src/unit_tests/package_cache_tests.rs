// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::move_cache::{MoveCache, Package},
    dev_utils::{
        compilation_utils::{compile_modules_in_file, compile_packages},
        in_memory_test_adapter::InMemoryTestAdapter,
        vm_test_adapter::VMTestAdapter,
    },
    natives::functions::NativeFunctions,
    runtime::{data_cache::TransactionDataCache, package_resolution::resolve_packages},
    shared::{linkage_context::LinkageContext, types::PackageStorageId},
};
use move_binary_format::errors::VMResult;
use move_core_types::{account_address::AccountAddress, resolver::MoveResolver};
use move_vm_config::runtime::VMConfig;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

fn load_linkage_packages_into_runtime<DataSource: MoveResolver + Send + Sync>(
    adapter: &mut impl VMTestAdapter<DataSource>,
    linkage: &LinkageContext,
) -> VMResult<BTreeMap<PackageStorageId, Arc<Package>>> {
    let cache = adapter.runtime().cache();
    let natives = adapter.runtime().natives();
    let vm_config = adapter.runtime().vm_config();
    let all_packages = linkage.all_packages()?;
    let storage = TransactionDataCache::new(adapter.storage());
    resolve_packages(
        &cache,
        &natives,
        &vm_config,
        &storage,
        linkage,
        all_packages,
    )
}

#[test]
fn cache_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package1.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.runtime.storage_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn cache_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package2.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.runtime.storage_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn cache_package_external_package_calls_no_types() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package3.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the packages correctly
    let results = result.unwrap();
    let l_pkg = results.get(&package1_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 2);
    assert_eq!(l_pkg.runtime.storage_id, package1_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 2);

    let l_pkg = results.get(&package2_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 1);
    assert_eq!(l_pkg.runtime.storage_id, package2_address);
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
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package1.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.runtime.storage_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
}

#[test]
fn load_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package1.move", &[]))
        .unwrap();
    let link_context = adapter.generate_default_linkage(package_address).unwrap();

    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.runtime.storage_id, package_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 3);
    assert_eq!(l_pkg.runtime.vtable.types.read().cached_types.len(), 0);
}

#[test]
fn load_package_external_package_calls_no_types() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package3.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 2);
    let l_pkg = l_pkg.get(&package2_address).unwrap();
    assert_eq!(l_pkg.runtime.loaded_modules.binaries.len(), 1);
    assert_eq!(l_pkg.runtime.storage_id, package2_address);
    assert_eq!(l_pkg.runtime.vtable.functions.len(), 1);

    for fptr in l_pkg.runtime.vtable.functions.binaries.iter() {
        println!("{:#?}", fptr.to_ref().code());
    }
}

#[test]
fn cache_package_external_package_type_references() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package4.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
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
    let linkage_context = adapter
        .generate_linkage_context(package1_address, package1_address, &a_pkg)
        .expect("Failed to generate linkage");
    adapter
        .publish_package_modules_for_test(linkage_context, package1_address, a_pkg)
        .unwrap();

    // publish b
    let linkage_context = adapter
        .generate_linkage_context(package2_address, package2_address, &b_pkg)
        .expect("Failed to generate linkage");
    adapter
        .publish_package_modules_for_test(
            linkage_context,
            package2_address,
            b_pkg,
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
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package4.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result1.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result2.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );

    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );
}

#[test]
fn cache_package_external_package_type_references_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package5.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package3_address).unwrap();
    let result = load_linkage_packages_into_runtime(&mut adapter, &link_context);

    assert!(result.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package3_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );

    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );

    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );
}

#[test]
fn cache_package_external_package_type_references_cache_reload_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package5.move", &[]))
        .unwrap();

    // Load from the bottom up
    let link_context = adapter.generate_default_linkage(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result1.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result2.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    let link_context = adapter.generate_default_linkage(package3_address).unwrap();
    let result3 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result3.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package3_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    // Now load it the other way -- from the top down. We do it in a new adapter to get a new
    // cache, etc., all set up.
    let mut adapter = InMemoryTestAdapter::new();
    adapter
        .insert_modules_into_storage(compile_modules_in_file("package5.move", &[]))
        .unwrap();

    let link_context = adapter.generate_default_linkage(package3_address).unwrap();
    let result3 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result3.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package3_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    let link_context = adapter.generate_default_linkage(package1_address).unwrap();
    let result1 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result1.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package3_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );

    let link_context = adapter.generate_default_linkage(package2_address).unwrap();
    let result2 = load_linkage_packages_into_runtime(&mut adapter, &link_context);
    assert!(result2.is_ok());
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package3_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package2_address]
            .read()
            .cached_types
            .id_map
            .len(),
        3
    );
    assert_eq!(
        adapter.runtime().cache().type_cache().read().package_cache[&package1_address]
            .read()
            .cached_types
            .id_map
            .len(),
        4
    );
}

#[test]
fn linkage_missing_dependency() {
    let mut adapter = InMemoryTestAdapter::new();
    let packages = compile_packages("rt_b_v0.move", &["rt_c_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .insert_modules_into_storage(modules.clone())
        .unwrap();
    // Linkage generation fails because we can't find the dependency.
    adapter
        .generate_linkage_context(runtime_package_id, runtime_package_id, &modules)
        .unwrap_err();
}

#[test]
fn linkage_unpublished_dependency() {
    let mut adapter = InMemoryTestAdapter::new();
    let packages = compile_packages("rt_b_v0.move", &["rt_c_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .insert_modules_into_storage(modules.clone())
        .unwrap();
    // Linkage generation fails because we can't find the dependency.
    adapter
        .generate_linkage_context(runtime_package_id, runtime_package_id, &modules)
        .unwrap_err();
}

#[test]
fn publish_missing_dependency() {
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    let packages = compile_packages(
        "rt_b_v0.move", /* 0x3::b */
        &["rt_c_v0.move" /* 0x2::c */],
    );

    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .insert_modules_into_storage(modules.clone())
        .unwrap();

    // Custom linkage because 0x2 is missing from the store and linkage generation would fail.
    let linkage_table = HashMap::from([(runtime_package_id, runtime_package_id)]);
    let linkage_context = LinkageContext::new(package3_address, linkage_table);

    // Publication fails because `0x2` is not in the linkage context.
    adapter
        .publish_package_modules_for_test(linkage_context, runtime_package_id, modules)
        .unwrap_err();
}

#[test]
fn publish_unpublished_dependency() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();
    let packages = compile_packages(
        "rt_b_v0.move", /* 0x3::b */
        &["rt_c_v0.move" /* 0x2::c */],
    );

    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .insert_modules_into_storage(modules.clone())
        .unwrap();

    // Custom linkage including `0x2 => 0x2`, which will cause publication to fail `0x3::b`.
    let linkage_table = HashMap::from([
        (runtime_package_id, runtime_package_id),
        (package2_address, package2_address),
    ]);
    let linkage_context = LinkageContext::new(package3_address, linkage_table);

    // Publication fails because `0x2` is not in the data cache.
    adapter
        .publish_package_modules_for_test(linkage_context, runtime_package_id, modules)
        .unwrap_err();
}

#[test]
fn publish_upgrade() {
    let v0_pkg_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let v1_pkg_address = AccountAddress::from_hex_literal("0x3").unwrap();

    let mut adapter = InMemoryTestAdapter::new();

    // First publish / linkage is the runtime package address to itself, because this is V0

    let (runtime_pkg_address, modules) = {
        let packages = compile_packages("rt_c_v0.move", &[]);
        assert!(packages.len() == 1);
        packages.into_iter().next().unwrap()
    };
    assert!(v0_pkg_address == runtime_pkg_address); // sanity

    let linkage_table = HashMap::from([(v0_pkg_address, v0_pkg_address)]);
    let linkage_context = LinkageContext::new(v0_pkg_address, linkage_table);
    adapter
        .publish_package_modules_for_test(
            linkage_context,
            /* runtime_id */ v0_pkg_address,
            modules,
        )
        .unwrap();

    // First publish / linkage is `0x3 => 0x2` for V1

    let (v0_pkg_address, modules) = {
        let packages = compile_packages("rt_c_v1.move", &[]);
        assert!(packages.len() == 1);
        packages.into_iter().next().unwrap()
    };

    let linkage_table = HashMap::from([(v0_pkg_address, v1_pkg_address)]);
    let linkage_context = LinkageContext::new(v1_pkg_address, linkage_table);
    adapter
        .publish_package_modules_for_test(
            linkage_context,
            /* runtime_id */ v0_pkg_address,
            modules,
        )
        .unwrap();
}

// Test that we properly publish and relink (and reuse) packages.
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
