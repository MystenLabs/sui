// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::loader::type_cache::TypeCache;
use crate::unit_tests::relinking_store::RelinkingStore;
use crate::{
    data_cache::TransactionDataCache,
    loader::{
        package_cache::PackageCache,
        package_loader::{LoadingPackage, PackageLoader},
    },
    native_functions::NativeFunctions,
};
use anyhow::Result;
use move_binary_format::file_format::CompiledModule;
use move_compiler::{
    compiled_unit::AnnotatedCompiledUnit,
    diagnostics::WarningFilters,
    editions::{Edition, Flavor},
    shared::PackageConfig,
    Compiler as MoveCompiler,
};
use move_core_types::{account_address::AccountAddress, resolver::ModuleResolver};
use move_vm_config::runtime::VMConfig;
use move_vm_test_utils::InMemoryStorage;
use parking_lot::RwLock;
use std::path::{Path, PathBuf};

pub fn expect_modules(
    units: impl IntoIterator<Item = AnnotatedCompiledUnit>,
) -> impl Iterator<Item = CompiledModule> {
    units
        .into_iter()
        .map(|annot_module| annot_module.named_module.module)
}

pub fn compile_modules_in_file(path: &Path) -> Result<Vec<CompiledModule>> {
    let (_, units) = MoveCompiler::from_files(
        None,
        vec![path.to_str().unwrap().to_string()],
        vec![],
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
    cache: PackageCache,
}

impl Adapter {
    fn new() -> Self {
        let storage = RelinkingStore::new(InMemoryStorage::new());
        let cache = PackageCache::new();
        let storage = TransactionDataCache::new(storage);
        Self { storage, cache }
    }

    fn compile_and_insert_packages_into_storage(
        &mut self,
        package_name: &str,
        root_address: AccountAddress,
    ) {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("src");
        path.push("unit_tests");
        path.push("packages");
        path.push(package_name);
        let modules = compile_modules_in_file(&path).unwrap();
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
}

#[test]
fn cache_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    // Get the package and make sure that we have what we expect
    let package = adapter
        .storage
        .get_remote_resolver()
        .get_package(&package_address);
    assert!(package.is_ok());
    assert!(package
        .as_ref()
        .unwrap()
        .as_ref()
        .is_some_and(|blobs| blobs.len() == 3));

    let modules = package
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|blob| {
            CompiledModule::deserialize_with_defaults(&blob).expect("Failed to deserialize module")
        })
        .collect();

    let type_cache = RwLock::new(TypeCache::new());

    let result = adapter.cache.cache_package(
        package_address,
        LoadingPackage::new(package_address, modules),
        &native_functions,
        &adapter.storage,
        &type_cache,
    );

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
}

#[test]
fn cache_package_internal_package_calls_only_with_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package2.move", package_address);

    // Get the package and make sure that we have what we expect
    let package = adapter
        .storage
        .get_remote_resolver()
        .get_package(&package_address);
    assert!(package.is_ok());
    assert!(package
        .as_ref()
        .unwrap()
        .as_ref()
        .is_some_and(|blobs| blobs.len() == 3));

    let modules = package
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|blob| {
            CompiledModule::deserialize_with_defaults(&blob).expect("Failed to deserialize module")
        })
        .collect();

    let type_cache = RwLock::new(TypeCache::new());

    let result = adapter.cache.cache_package(
        package_address,
        LoadingPackage::new(package_address, modules),
        &native_functions,
        &adapter.storage,
        &type_cache,
    );

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
    println!("{:#?}", type_cache.read().cached_types.id_map);
}

#[test]
fn cache_package_external_package_calls_no_types() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package3.move", package1_address);

    // Get the package and make sure that we have what we expect
    let package1 = adapter
        .storage
        .get_remote_resolver()
        .get_package(&package1_address);
    assert!(package1.is_ok());
    assert!(package1
        .as_ref()
        .unwrap()
        .as_ref()
        .is_some_and(|blobs| blobs.len() == 2));

    let package2 = adapter
        .storage
        .get_remote_resolver()
        .get_package(&package2_address);
    assert!(package2.is_ok());
    assert!(package2
        .as_ref()
        .unwrap()
        .as_ref()
        .is_some_and(|blobs| blobs.len() == 1));

    let modules1 = package1
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|blob| {
            CompiledModule::deserialize_with_defaults(&blob).expect("Failed to deserialize module")
        })
        .collect();

    let modules2 = package2
        .unwrap()
        .unwrap()
        .into_iter()
        .map(|blob| {
            CompiledModule::deserialize_with_defaults(&blob).expect("Failed to deserialize module")
        })
        .collect();

    let type_cache = RwLock::new(TypeCache::new());
    let result1 = adapter.cache.cache_package(
        package1_address,
        LoadingPackage::new(package1_address, modules1),
        &native_functions,
        &adapter.storage,
        &type_cache,
    );

    // Verify that we've loaded the package correctly
    let l_pkg = result1.unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 2);
    assert_eq!(l_pkg.storage_id, package1_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 2);

    let result2 = adapter.cache.cache_package(
        package2_address,
        LoadingPackage::new(package2_address, modules2),
        &native_functions,
        &adapter.storage,
        &type_cache,
    );
    println!("{:#?}", result2.is_ok());
}

#[test]
fn load_package_internal_package_calls_only_no_types() {
    let package_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());
    let result = loader.load_and_cache_link_context(&adapter.storage);

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
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package1.move", package_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());
    let result = loader.load_and_cache_link_context(&adapter.storage);

    // Verify that we've loaded the package correctly
    let l_pkg = result.unwrap();
    assert_eq!(l_pkg.len(), 1);
    let l_pkg = l_pkg.get(&package_address).unwrap();
    assert_eq!(l_pkg.loaded_modules.binaries.len(), 3);
    assert_eq!(l_pkg.storage_id, package_address);
    assert_eq!(l_pkg.vtable.binaries.len(), 3);
    println!("{:#?}", loader.type_cache.read().cached_types.id_map);
}

#[test]
fn load_package_external_package_calls_no_types() {
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package3.move", package2_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());

    let result = loader.load_and_cache_link_context(&adapter.storage);

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
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package4.move", package2_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());

    let result1 = loader.load_and_cache_link_context(&adapter.storage);

    assert!(result1.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 7);
}

#[test]
fn cache_package_external_package_type_references_cache_reload() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package4.move", package1_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());

    let result1 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 4);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 7);
}

#[test]
fn cache_package_external_package_type_references_with_shared_dep() {
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package5.move", package3_address);

    let loader = PackageLoader::new(native_functions, VMConfig::default());
    let result = loader.load_and_cache_link_context(&adapter.storage);

    assert!(result.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 10);
}

#[test]
fn cache_package_external_package_type_references_cache_reload_with_shared_dep() {
    let package1_address = AccountAddress::from_hex_literal("0x1").unwrap();
    let package2_address = AccountAddress::from_hex_literal("0x2").unwrap();
    let package3_address = AccountAddress::from_hex_literal("0x3").unwrap();
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let mut adapter = Adapter::new();
    adapter.compile_and_insert_packages_into_storage("package5.move", package1_address);

    // Load from the bottom up
    let loader = PackageLoader::new(native_functions, VMConfig::default());
    let result1 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 4);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 7);

    adapter.storage.get_remote_resolver_mut().context = package3_address;
    let result3 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result3.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 10);

    // Now load it the other way -- from the top down
    let native_functions = NativeFunctions::new(vec![]).unwrap();
    let loader = PackageLoader::new(native_functions, VMConfig::default());

    adapter.storage.get_remote_resolver_mut().context = package3_address;
    let result3 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result3.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 10);

    adapter.storage.get_remote_resolver_mut().context = package1_address;
    let result1 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result1.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 10);

    adapter.storage.get_remote_resolver_mut().context = package2_address;
    let result2 = loader.load_and_cache_link_context(&adapter.storage);
    assert!(result2.is_ok());
    assert_eq!(loader.type_cache.read().cached_types.id_map.len(), 10);
}
