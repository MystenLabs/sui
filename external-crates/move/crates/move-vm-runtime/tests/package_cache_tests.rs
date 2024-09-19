// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

mod relinking_store;

use anyhow::Result;
use move_binary_format::errors::VMResult;
use move_binary_format::file_format::CompiledModule;
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::WarningFilters,
    editions::{Edition, Flavor},
    shared::PackageConfig,
    Compiler as MoveCompiler,
};
use move_core_types::account_address::AccountAddress;
use move_core_types::identifier::Identifier;
use move_core_types::language_storage::ModuleId;
use move_vm_config::runtime::VMConfig;
use move_vm_runtime::on_chain::ast::{PackageStorageId, RuntimePackageId};
use move_vm_runtime::{
    cache::vm_cache::VMCache, natives::functions::NativeFunctions,
    on_chain::data_cache::TransactionDataCache,
};
use move_vm_test_utils::InMemoryStorage;
use relinking_store::RelinkingStore;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn expect_modules(
    units: impl IntoIterator<Item = AnnotatedCompiledUnit>,
) -> impl Iterator<Item = CompiledModule> {
    units
        .into_iter()
        .map(|annot_module| annot_module.named_module.module)
}

pub fn compile_modules_in_file(path: &Path, deps: Vec<String>) -> Result<Vec<CompiledModule>> {
    let (_, units) = MoveCompiler::from_files(
        None,
        vec![path.to_str().unwrap().to_string()],
        deps,
        std::collections::BTreeMap::<String, _>::new(),
    )
    .set_default_config(PackageConfig {
        is_dependency: false,
        warning_filter: WarningFilters::new_for_source(),
        flavor: Flavor::Sui,
        edition: Edition::E2024_ALPHA,
    })
    .build_and_report()?;

    Ok(expect_modules(units).collect())
}

struct Adapter {
    storage: TransactionDataCache<RelinkingStore>,
    cache: VMCache,
}

impl Adapter {
    fn new() -> Self {
        let storage = RelinkingStore::new(InMemoryStorage::new());
        let native_functions = Arc::new(NativeFunctions::empty_for_testing().unwrap());
        let vm_config = Arc::new(VMConfig::default());
        let cache = VMCache::new(native_functions, vm_config);
        let storage = TransactionDataCache::new(storage);
        Self { storage, cache }
    }

    fn make_base_path() -> PathBuf {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("tests");
        path.push("move_packages");
        path
    }

    fn compile_and_insert_packages_into_storage(
        &mut self,
        package_name: &str,
        root_address: AccountAddress,
    ) {
        let mut path = Adapter::make_base_path();
        path.push(package_name);
        let modules = compile_modules_in_file(&path, vec![]).unwrap();
        assert!(!modules.is_empty(), "Tried to publish an empty package");
        for module in modules {
            let module_id = module.self_id();
            let mut module_bytes = vec![];
            module
                .serialize_with_version(module.version, &mut module_bytes)
                .unwrap();
            self.storage
                .get_remote_resolver_mut()
                .linkage
                .insert(module_id.clone(), module_id.clone());
            self.storage
                .get_remote_resolver_mut()
                .store
                .publish_or_overwrite_module(module_id, module_bytes);
        }

        self.storage.get_remote_resolver_mut().context = root_address;
    }

    fn compile_packages(
        &mut self,
        package_name: &str,
        dependencies: &[&str],
    ) -> BTreeMap<RuntimePackageId, Vec<CompiledModule>> {
        let mut path = Adapter::make_base_path();
        path.push(package_name);
        let deps = dependencies
            .iter()
            .map(|dep| {
                let mut path = Adapter::make_base_path();
                path.push(dep);
                path.to_string_lossy().to_string()
            })
            .collect();
        let modules = compile_modules_in_file(&path, deps).unwrap();
        assert!(!modules.is_empty(), "Tried to publish an empty package");
        let mut packages = BTreeMap::new();
        for module in modules {
            let module_id = module.self_id();
            packages
                .entry(*module_id.address())
                .or_insert_with(Vec::new)
                .push(module);
        }

        packages
    }

    fn publish_package(
        &mut self,
        runtime_package_id: RuntimePackageId,
        storage_id: PackageStorageId,
        modules: Vec<CompiledModule>,
        type_origin: BTreeMap<(ModuleId, Identifier), ModuleId>,
        dependencies: BTreeSet<PackageStorageId>,
    ) -> VMResult<()> {
        let remote_resolver = self.storage.get_remote_resolver_mut();
        let linkage = modules
            .iter()
            .map(|module| {
                (
                    module.self_id(),
                    ModuleId::new(storage_id, module.self_id().name().to_owned()),
                )
            })
            .collect();
        remote_resolver.relink(storage_id, linkage, type_origin);
        remote_resolver.set_dependent_packages(dependencies);
        let modules = modules
            .into_iter()
            .map(|module| {
                let mut module_bytes = vec![];
                module
                    .serialize_with_version(module.version, &mut module_bytes)
                    .unwrap();
                module_bytes
            })
            .collect();
        self.cache
            .verify_package_for_publication(modules, &self.storage, runtime_package_id)
    }
}

#[test]
fn cache_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    let result = adapter.cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
}

#[test]
fn cache_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package2.move", package_address);

    let result = adapter.cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let result = result.unwrap();
    let l_pkg = result.first_key_value().unwrap().1;
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
    println!(
        "{:#?}",
        adapter.cache.type_cache().read().cached_types.id_map
    );
}

#[test]
fn cache_package_external_package_calls_no_types() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package3.move", package2_address);
    let result = adapter.cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the packages correctly
    let results = result.unwrap();
    let l_pkg = results.get(&package1_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 2);
    assert_eq!(l_pkg.storage_id, package1_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 2);

    let l_pkg = results.get(&package2_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 1);
    assert_eq!(l_pkg.storage_id, package2_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 1);
}

/// Generate a new, dummy cachce for testing.
fn dummy_cache_for_testing() -> VMCache {
    let native_functions = Arc::new(NativeFunctions::new(vec![]).unwrap());
    let config = Arc::new(VMConfig::default());
    VMCache::new(native_functions, config)
}

#[test]
fn load_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    let cache = dummy_cache_for_testing();
    let result = cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
}

#[test]
fn load_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    let cache = dummy_cache_for_testing();
    let result = cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
    println!("{:#?}", cache.type_cache().read().cached_types.id_map);
}

#[test]
fn load_package_external_package_calls_no_types() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package3.move", package2_address);

    let cache = dummy_cache_for_testing();

    let result = cache.resolve_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 2);
    let l_pkg = l_pkg.get(&package2_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 1);
    assert_eq!(l_pkg.storage_id, package2_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 1);

    for fptr in l_pkg.vtable.binaries.iter() {
        println!("{:#?}", fptr.to_ref().code());
    }
}

#[test]
fn cache_package_external_package_type_references() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package4.move", package2_address);

    let cache = dummy_cache_for_testing();

    let result1 = cache.resolve_link_context(&adapter.storage);

    assert!(result1.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 7);
}

#[test]
fn cache_package_external_generic_call_type_references() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = Adapter::new();

    let mut packages = adapter.compile_packages("package6.move", &[]);

    let a_pkg = packages.remove(&package1_address).unwrap();
    let b_pkg = packages.remove(&package2_address).unwrap();

    // publish a
    adapter
        .publish_package(
            package1_address,
            package1_address,
            a_pkg,
            BTreeMap::new(),
            BTreeSet::new(),
        )
        .unwrap();

    // publish b
    adapter
        .publish_package(
            package2_address,
            package2_address,
            b_pkg,
            [(
                (
                    ModuleId::new(package1_address, Identifier::new("a").unwrap()),
                    Identifier::new("AA").unwrap(),
                ),
                ModuleId::new(package1_address, Identifier::new("a").unwrap()),
            )]
            .into_iter()
            .collect(),
            [package1_address].into_iter().collect(),
        )
        .unwrap();
}

#[test]
fn cache_package_external_package_type_references_cache_reload() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package4.move", package1_address);

    let cache = dummy_cache_for_testing();

    let result1 = cache.resolve_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 4);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = cache.resolve_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 7);
}

#[test]
fn cache_package_external_package_type_references_with_shared_dep() {
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package5.move", package3_address);

    let cache = dummy_cache_for_testing();
    let result = cache.resolve_link_context(&adapter.storage);

    assert!(result.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 10);
}

#[test]
fn cache_package_external_package_type_references_cache_reload_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package5.move", package1_address);

    // Load from the bottom up
    let cache = dummy_cache_for_testing();
    let result1 = cache.resolve_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 4);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = cache.resolve_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 7);

    adapter.storage.get_remote_resolver_mut().context = package3_address;
    let result3 = cache.resolve_link_context(&adapter.storage);
    assert!(result3.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 10);

    // Now load it the other way -- from the top down
    let cache = dummy_cache_for_testing();

    adapter.storage.get_remote_resolver_mut().context = package3_address;
    let result3 = cache.resolve_link_context(&adapter.storage);
    assert!(result3.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 10);

    adapter.storage.get_remote_resolver_mut().context = package1_address;
    let result1 = cache.resolve_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 10);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = cache.resolve_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(cache.type_cache().read().cached_types.id_map.len(), 10);
}

#[test]
fn publish_missing_dependency() {
    let mut adapter = Adapter::new();
    let packages = adapter.compile_packages("rt_b_v0.move", &["rt_c_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            BTreeSet::new(),
        )
        .unwrap_err();
}

#[test]
fn publish_unpublished_dependency() {
    let mut adapter = Adapter::new();
    let packages = adapter.compile_packages("rt_b_v0.move", &["rt_c_v0.move"]);
    let c_runtime_addr = AccountAddress::from_hex_literal("0x2").unwrap();
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            [c_runtime_addr].into_iter().collect(),
        )
        .unwrap_err();
}

// Test that we properly publish and relink (and reuse) packages.
#[test]
fn relink() {
    let mut adapter = Adapter::new();

    let st_c_v1_addr = AccountAddress::from_hex_literal("0x42").unwrap();
    let st_b_v1_addr = AccountAddress::from_hex_literal("0x43").unwrap();

    let c_runtime_addr = AccountAddress::from_hex_literal("0x2").unwrap();
    let b_runtime_addr = AccountAddress::from_hex_literal("0x3").unwrap();
    let _a_runtime_addr = AccountAddress::from_hex_literal("0x4").unwrap();

    // publish c v0
    let packages = adapter.compile_packages("rt_c_v0.move", &[]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            BTreeSet::new(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 1);

    // publish c v1
    let packages = adapter.compile_packages("rt_c_v1.move", &[]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
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

    assert_eq!(adapter.cache.package_cache().read().len(), 2);

    // publish b_v0 <- c_v0
    let packages = adapter.compile_packages("rt_b_v0.move", &["rt_c_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
            runtime_package_id,
            runtime_package_id,
            modules,
            BTreeMap::new(),
            [c_runtime_addr].into_iter().collect(),
        )
        .unwrap();

    assert_eq!(adapter.cache.package_cache().read().len(), 3);

    // publish b_v0 <- c_v1
    let packages = adapter.compile_packages("rt_b_v0.move", &["rt_c_v1.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
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
    let packages = adapter.compile_packages("rt_a_v0.move", &["rt_c_v1.move", "rt_b_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
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
    let packages = adapter.compile_packages("rt_a_v0.move", &["rt_c_v1.move", "rt_b_v0.move"]);
    assert!(packages.len() == 1);
    let (runtime_package_id, modules) = packages.into_iter().next().unwrap();
    adapter
        .publish_package(
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
}
