// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    cache::identifier_interner::IdentifierInterner,
    dev_utils::{
        compilation_utils::{compile_packages_in_file, compile_units, expect_modules},
        storage::StoredPackage,
    },
    jit::{execution::ast::*, translate_package},
    natives::functions::NativeFunctions,
    shared::TypeLimits,
    validation::verification::ast as verified,
};
use move_binary_format::CompiledModule;
use move_core_types::account_address::AccountAddress;
use move_vm_config::runtime::VMConfig;
use std::{collections::BTreeMap, hint::black_box, mem::size_of, time::Instant};

fn verified_package(stored: StoredPackage) -> verified::Package {
    let stored = stored.0;
    let modules = stored
        .modules
        .into_values()
        .map(|bytes| {
            let module = CompiledModule::deserialize_with_defaults(&bytes).unwrap();
            (module.self_id(), verified::Module { value: module })
        })
        .collect();
    verified::Package {
        original_id: stored.original_id,
        version_id: stored.version_id,
        modules,
        type_origin_table: stored.type_origin_table,
        linkage_table: stored.linkage_table,
        version: stored.version,
    }
}

fn translate(verified: verified::Package, names: &IdentifierInterner) -> Package {
    translate_package(
        &VMConfig::new_for_test(false, None),
        &TypeLimits::VM_DEFAULT,
        names,
        &NativeFunctions::empty_for_testing().unwrap(),
        &BTreeMap::new(),
        verified,
    )
    .unwrap()
}

fn synthetic_package(repeats: usize) -> verified::Package {
    let mut source = String::from(
        "module 0x3::external { public fun ping() {} }\nmodule 0x2::keys {\nuse 0x3::external;\npublic struct S has copy, drop { value: u64 }\n",
    );
    for index in 0..repeats {
        source.push_str(&format!(
            "public fun f{index}(s: &S): u64 {{ external::ping(); external::ping(); s.value }}\n"
        ));
    }
    source.push('}');
    let modules = expect_modules(compile_units(&source).unwrap())
        .filter(|module| *module.self_id().address() == AccountAddress::TWO)
        .collect();
    verified_package(StoredPackage::from_modules_for_testing(AccountAddress::TWO, modules).unwrap())
}

#[test]
fn compiled_keys_share_storage_and_runtime_types_own_their_keys() {
    let names = IdentifierInterner::new();
    let package = translate(synthetic_package(2), &names);
    let module = package.loaded_modules.first().unwrap().1;
    let struct_key = &module.structs[0].def_vtable_key;
    let calls = module
        .functions
        .iter()
        .flat_map(|function| function.code.iter())
        .filter_map(|instruction| match instruction {
            Bytecode::VirtualCall(key) => Some(key),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(calls.len(), 4);
    for call in &calls[1..] {
        assert!(calls[0].ptr_eq(call));
    }
    for field in module.field_handles.iter() {
        assert!(struct_key.ptr_eq(&field.owner));
    }
    for function in module.functions.iter() {
        let ArenaType::Reference(inner) = function.parameters[0].ty.to_ref() else {
            panic!("expected a reference parameter");
        };
        let ArenaType::Datatype(key) = inner.to_ref() else {
            panic!("expected a datatype parameter");
        };
        assert!(struct_key.ptr_eq(key));
    }
    let runtime_type = module.structs[0].datatype();
    let copied_key = struct_key.to_ref().clone();
    drop(package);
    assert_eq!(runtime_type, Type::Datatype(copied_key));
}

#[test]
#[ignore]
fn measure_vtable_key_translation() {
    println!(
        "SIZES key={} pointer={} arena_type={} bytecode={} function={} struct_def={} enum_def={} field_handle={} field_instantiation={} struct_instantiation={} enum_instantiation={}",
        size_of::<crate::execution::dispatch_tables::VirtualTableKey>(),
        size_of::<
            crate::shared::vm_pointer::VMPointer<
                crate::execution::dispatch_tables::VirtualTableKey,
            >,
        >(),
        size_of::<ArenaType>(),
        size_of::<Bytecode>(),
        size_of::<Function>(),
        size_of::<StructDef>(),
        size_of::<EnumDef>(),
        size_of::<FieldHandle>(),
        size_of::<FieldInstantiation>(),
        size_of::<StructInstantiation>(),
        size_of::<EnumInstantiation>()
    );
    let mut cases = Vec::new();
    for fixture in [
        "package2.move",
        "package4.move",
        "depth_tests_modules.move",
        "loader_tests_modules.move",
    ] {
        let packages = compile_packages_in_file(fixture, &[])
            .into_iter()
            .map(verified_package)
            .collect::<Vec<_>>();
        cases.push((fixture.to_string(), packages));
    }
    for repeats in [1, 64, 256] {
        cases.push((
            format!("synthetic_{repeats}"),
            vec![synthetic_package(repeats)],
        ));
    }
    for (name, packages) in cases {
        let names = IdentifierInterner::new();
        let natives = NativeFunctions::empty_for_testing().unwrap();
        let config = VMConfig::new_for_test(false, None);
        let system = BTreeMap::new();
        let translate = |package| {
            translate_package(
                &config,
                &TypeLimits::VM_DEFAULT,
                &names,
                &natives,
                &system,
                package,
            )
            .unwrap()
        };
        let arena_bytes: usize = packages
            .iter()
            .map(|package| translate(package.clone()).package_arena.allocated_bytes())
            .sum();
        for _ in 0..10 {
            for package in &packages {
                black_box(translate(package.clone()));
            }
        }
        for sample in 0..25 {
            let start = Instant::now();
            for _ in 0..20 {
                for package in &packages {
                    black_box(translate(package.clone()));
                }
            }
            println!(
                "SAMPLE case={name} sample={sample} ns={} arena_bytes={arena_bytes}",
                start.elapsed().as_nanos() / 20
            );
        }
    }
}
