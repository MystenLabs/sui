// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

//! Tests that missing transitive dependencies are correctly caught by both
//! `validate_against_link_context` (cardinality check between the resolved package set and the
//! linkage table) and `verify_linkage_and_cyclic_checks` / its publication variant (module-level
//! dependency resolution).

use crate::{
    shared::{
        linkage_context::LinkageContext,
        types::{OriginalId, VersionId},
    },
    validation::{
        self,
        verification::{
            ast::{Module, Package},
            linkage::{
                verify_linkage_and_cyclic_checks, verify_linkage_and_cyclic_checks_for_publication,
            },
        },
    },
};
use indexmap::IndexMap;
use move_binary_format::{
    file_format::{AddressIdentifierIndex, IdentifierIndex, ModuleHandle, ModuleHandleIndex},
    CompiledModule,
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use std::collections::BTreeMap;

// ------------------------------------------------------------------------------------------------
// Helpers
// ------------------------------------------------------------------------------------------------

fn addr(n: u64) -> AccountAddress {
    let mut bytes = [0u8; AccountAddress::LENGTH];
    bytes[AccountAddress::LENGTH - 8..].copy_from_slice(&n.to_be_bytes());
    AccountAddress::new(bytes)
}

fn dummy_package(original_id: OriginalId, version_id: VersionId) -> Package {
    Package {
        original_id,
        version_id,
        modules: BTreeMap::new(),
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::new(),
        version: 1,
    }
}

/// Build a minimal `CompiledModule` at `self_addr::self_name` whose module-handle table
/// references each entry in `deps`, making them appear as immediate dependencies.
fn make_module(
    self_addr: AccountAddress,
    self_name: &str,
    deps: &[(AccountAddress, &str)],
) -> CompiledModule {
    let mut address_identifiers = vec![self_addr];
    let mut identifiers: Vec<Identifier> = vec![Identifier::new(self_name).unwrap()];
    let mut module_handles = vec![ModuleHandle {
        address: AddressIdentifierIndex(0),
        name: IdentifierIndex(0),
    }];

    for (dep_addr, dep_name) in deps {
        let addr_idx = address_identifiers
            .iter()
            .position(|a| a == dep_addr)
            .unwrap_or_else(|| {
                address_identifiers.push(*dep_addr);
                address_identifiers.len() - 1
            });
        let name_idx = {
            identifiers.push(Identifier::new(*dep_name).unwrap());
            identifiers.len() - 1
        };
        module_handles.push(ModuleHandle {
            address: AddressIdentifierIndex(addr_idx as u16),
            name: IdentifierIndex(name_idx as u16),
        });
    }

    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        publishable: true,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles,
        address_identifiers,
        identifiers,
        datatype_handles: vec![],
        function_handles: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![],
        constant_pool: vec![],
        metadata: vec![],
        struct_defs: vec![],
        function_defs: vec![],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    }
}

fn wrap_module(compiled: CompiledModule) -> Module {
    Module { value: compiled }
}

fn make_package_with_modules(
    original_id: AccountAddress,
    version_id: AccountAddress,
    modules: Vec<Module>,
) -> Package {
    let module_map = modules
        .into_iter()
        .map(|m| (m.value.self_id(), m))
        .collect();
    Package {
        original_id,
        version_id,
        modules: module_map,
        type_origin_table: IndexMap::new(),
        linkage_table: BTreeMap::new(),
        version: 1,
    }
}

// ================================================================================================
// validate_against_link_context — cardinality / linkage-table completeness
// ================================================================================================

// --- happy paths ---

#[test]
fn validate_link_context_ok_single_package() {
    let pkg = dummy_package(addr(1), addr(1));
    let packages = BTreeMap::from([(addr(1), &pkg)]);
    let link = LinkageContext::new(BTreeMap::from([(addr(1), addr(1))])).unwrap();
    assert!(validation::validate_against_link_context(false, &packages, &link).is_ok());
}

#[test]
fn validate_link_context_ok_multiple_packages() {
    let pkg_a = dummy_package(addr(1), addr(1));
    let pkg_b = dummy_package(addr(2), addr(2));
    let pkg_c = dummy_package(addr(3), addr(3));
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b), (addr(3), &pkg_c)]);
    let link = LinkageContext::new(BTreeMap::from([
        (addr(1), addr(1)),
        (addr(2), addr(2)),
        (addr(3), addr(3)),
    ]))
    .unwrap();
    assert!(validation::validate_against_link_context(false, &packages, &link).is_ok());
}

#[test]
fn validate_link_context_ok_publish() {
    // During publish the to-be-published package isn't in `packages` but has a linkage entry.
    let dep = dummy_package(addr(1), addr(1));
    let packages = BTreeMap::from([(addr(1), &dep)]);
    let link =
        LinkageContext::new(BTreeMap::from([(addr(1), addr(1)), (addr(2), addr(2))])).unwrap();
    assert!(validation::validate_against_link_context(true, &packages, &link).is_ok());
}

// --- missing transitive deps ---

#[test]
fn validate_link_context_err_missing_transitive_dep() {
    // A -> B -> C. Packages resolved include all three, but the linkage context
    // only has A and B -- C's entry is missing (incomplete serialized linkage).
    let pkg_a = dummy_package(addr(1), addr(1));
    let pkg_b = dummy_package(addr(2), addr(2));
    let pkg_c = dummy_package(addr(3), addr(3));
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b), (addr(3), &pkg_c)]);
    let link =
        LinkageContext::new(BTreeMap::from([(addr(1), addr(1)), (addr(2), addr(2))])).unwrap();
    let err = validation::validate_against_link_context(false, &packages, &link).unwrap_err();
    let msg = err.message().unwrap();
    assert!(msg.contains("2"), "expected 2 linkage entries, got: {msg}");
    assert!(msg.contains("3"), "expected 3 packages, got: {msg}");
}

#[test]
fn validate_link_context_err_missing_only_dep() {
    // A depends on B. Only A is in packages, but the linkage context is empty --
    // no dependency entries at all.
    let pkg = dummy_package(addr(1), addr(1));
    let packages = BTreeMap::from([(addr(1), &pkg)]);
    let link = LinkageContext::new(BTreeMap::new()).unwrap();
    let err = validation::validate_against_link_context(false, &packages, &link).unwrap_err();
    let msg = err.message().unwrap();
    assert!(msg.contains("0"), "linkage has 0 entries, got: {msg}");
    assert!(msg.contains("1"), "expected 1 package, got: {msg}");
}

#[test]
fn validate_link_context_err_missing_dep_during_publish() {
    // Publishing package B (0x2). Dep A (0x1) is in packages, but the linkage
    // context only has B's own entry -- A's transitive entry is missing.
    let dep = dummy_package(addr(1), addr(1));
    let packages = BTreeMap::from([(addr(1), &dep)]);
    let link = LinkageContext::new(BTreeMap::from([(addr(2), addr(2))])).unwrap();
    let err = validation::validate_against_link_context(true, &packages, &link).unwrap_err();
    let msg = err.message().unwrap();
    assert!(msg.contains("1"), "linkage has 1 entry, got: {msg}");
    assert!(msg.contains("2"), "expected 2 entries, got: {msg}");
}

#[test]
fn validate_link_context_err_missing_deep_transitive_dep() {
    // A -> B -> C -> D. Linkage context has A, B, C but not D.
    let pkg_a = dummy_package(addr(1), addr(1));
    let pkg_b = dummy_package(addr(2), addr(2));
    let pkg_c = dummy_package(addr(3), addr(3));
    let pkg_d = dummy_package(addr(4), addr(4));
    let packages = BTreeMap::from([
        (addr(1), &pkg_a),
        (addr(2), &pkg_b),
        (addr(3), &pkg_c),
        (addr(4), &pkg_d),
    ]);
    let link = LinkageContext::new(BTreeMap::from([
        (addr(1), addr(1)),
        (addr(2), addr(2)),
        (addr(3), addr(3)),
    ]))
    .unwrap();
    let err = validation::validate_against_link_context(false, &packages, &link).unwrap_err();
    let msg = err.message().unwrap();
    assert!(msg.contains("3"), "linkage has 3 entries, got: {msg}");
    assert!(msg.contains("4"), "expected 4 packages, got: {msg}");
}

// ================================================================================================
// verify_linkage_and_cyclic_checks — module-level dependency resolution
// ================================================================================================

// --- happy paths ---

#[test]
fn linkage_ok_no_deps() {
    let m = make_module(addr(1), "a", &[]);
    let pkg = make_package_with_modules(addr(1), addr(1), vec![wrap_module(m)]);
    let packages = BTreeMap::from([(addr(1), &pkg)]);
    assert!(verify_linkage_and_cyclic_checks(&packages).is_ok());
}

#[test]
fn linkage_ok_with_dep() {
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b)]);
    assert!(verify_linkage_and_cyclic_checks(&packages).is_ok());
}

#[test]
fn linkage_ok_transitive_chain() {
    // A -> B -> C, all present.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[(addr(3), "c")]);
    let mod_c = make_module(addr(3), "c", &[]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let pkg_c = make_package_with_modules(addr(3), addr(3), vec![wrap_module(mod_c)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b), (addr(3), &pkg_c)]);
    assert!(verify_linkage_and_cyclic_checks(&packages).is_ok());
}

#[test]
fn publication_ok_transitive_deps_present() {
    // Publishing A. A -> B -> C, both B and C in cache.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[(addr(3), "c")]);
    let mod_c = make_module(addr(3), "c", &[]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let pkg_c = make_package_with_modules(addr(3), addr(3), vec![wrap_module(mod_c)]);
    let cached = BTreeMap::from([(addr(2), &pkg_b), (addr(3), &pkg_c)]);
    assert!(verify_linkage_and_cyclic_checks_for_publication(&pkg_a, &cached).is_ok());
}

// --- missing transitive deps ---

#[test]
fn linkage_err_immediate_dep_missing() {
    // A -> B, but B's package is absent from the set.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a)]);
    let err = verify_linkage_and_cyclic_checks(&packages).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}

#[test]
fn linkage_err_transitive_dep_missing() {
    // A -> B -> C. A and B present, C missing.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[(addr(3), "c")]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b)]);
    let err = verify_linkage_and_cyclic_checks(&packages).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}

#[test]
fn linkage_err_one_of_multiple_transitive_deps_missing() {
    // A depends on B and C. B is present, C is not.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b"), (addr(3), "c")]);
    let mod_b = make_module(addr(2), "b", &[]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b)]);
    let err = verify_linkage_and_cyclic_checks(&packages).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}

#[test]
fn linkage_err_deep_transitive_dep_missing() {
    // A -> B -> C -> D. A, B, C present; D missing.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[(addr(3), "c")]);
    let mod_c = make_module(addr(3), "c", &[(addr(4), "d")]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let pkg_c = make_package_with_modules(addr(3), addr(3), vec![wrap_module(mod_c)]);
    let packages = BTreeMap::from([(addr(1), &pkg_a), (addr(2), &pkg_b), (addr(3), &pkg_c)]);
    let err = verify_linkage_and_cyclic_checks(&packages).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}

// --- missing transitive deps via publication path ---

#[test]
fn publication_err_immediate_dep_missing() {
    // Publishing A which depends on B, but B is not in cached_packages.
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let cached: BTreeMap<VersionId, &Package> = BTreeMap::new();
    let err = verify_linkage_and_cyclic_checks_for_publication(&pkg_a, &cached).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}

#[test]
fn publication_err_transitive_dep_missing() {
    // Publishing A. A -> B (present) -> C (missing).
    let mod_a = make_module(addr(1), "a", &[(addr(2), "b")]);
    let mod_b = make_module(addr(2), "b", &[(addr(3), "c")]);
    let pkg_a = make_package_with_modules(addr(1), addr(1), vec![wrap_module(mod_a)]);
    let pkg_b = make_package_with_modules(addr(2), addr(2), vec![wrap_module(mod_b)]);
    let cached = BTreeMap::from([(addr(2), &pkg_b)]);
    let err = verify_linkage_and_cyclic_checks_for_publication(&pkg_a, &cached).unwrap_err();
    assert_eq!(
        err.major_status(),
        move_core_types::vm_status::StatusCode::MISSING_DEPENDENCY,
    );
}
