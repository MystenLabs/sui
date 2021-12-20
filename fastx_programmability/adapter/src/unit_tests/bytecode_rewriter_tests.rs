// Copyright (c) Mysten Labs
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::{
    self, AbilitySet, FunctionHandle, SignatureIndex, StructHandle,
};
use move_core_types::{account_address::AccountAddress, ident_str, identifier::IdentStr};

use super::*;

fn make_id(addr: u8, name: &IdentStr) -> ModuleId {
    ModuleId::new(
        AccountAddress::new([addr; AccountAddress::LENGTH]),
        name.to_owned(),
    )
}

fn make_struct_handle(module: ModuleHandleIndex) -> StructHandle {
    StructHandle {
        module,
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    }
}

fn make_function_handle(module: ModuleHandleIndex) -> FunctionHandle {
    FunctionHandle {
        module,
        name: IdentifierIndex(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    }
}

fn make_module_handle(id: &ModuleId, m: &mut CompiledModule) -> ModuleHandle {
    ModuleHandle {
        address: ModuleHandleRewriter::get_or_create_address(id.address(), m),
        name: ModuleHandleRewriter::get_or_create_identifier(id.name(), m),
    }
}

#[test]
fn add_address_and_identifier() {
    // check that adding new addresses and identifier's to the module's table works
    let id = make_id(12, ident_str!("Name"));
    let addr = id.address();
    let name = id.name();

    // add address
    let mut m = file_format::empty_module();
    let old_addrs_len = m.address_identifiers.len();
    assert!(!m.address_identifiers.contains(addr));

    let addr_idx = ModuleHandleRewriter::get_or_create_address(addr, &mut m);
    assert!(m.address_identifiers.len() == old_addrs_len + 1);
    assert!(m.address_identifier_at(addr_idx) == addr);
    // after addition, should look up existing index instead of adding a new one
    assert!(ModuleHandleRewriter::get_or_create_address(addr, &mut m) == addr_idx);

    // add identifier
    let old_ids_len = m.identifiers.len();
    assert!(!m.identifiers.contains(&name.to_owned()));

    let id_idx = ModuleHandleRewriter::get_or_create_identifier(name, &mut m);
    assert!(m.identifiers.len() == old_ids_len + 1);
    assert!(m.identifier_at(id_idx) == name);
    // after addition, should look up existing index instead of adding a new one
    assert!(ModuleHandleRewriter::get_or_create_identifier(name, &mut m) == id_idx);
}

#[test]
fn add_module_handle() {
    let id = make_id(12, ident_str!("Name"));

    let mut m = file_format::empty_module();
    let old_handles_len = m.module_handles.len();
    let module_idx = ModuleHandleRewriter::get_or_create_module_handle(&id, &mut m);
    assert!(m.module_handles.len() == old_handles_len + 1);
    assert!(m.module_id_for_handle(m.module_handle_at(module_idx)) == id);
    // after addition, should look up existing handle instead of adding a new one
    assert!(ModuleHandleRewriter::get_or_create_module_handle(&id, &mut m) == module_idx);
}

// Check enforcement of the internal "sub map domain and range are disjoint" invariant
#[test]
fn test_disjoint_domain_range() {
    let id1 = make_id(0, ident_str!("Name1"));
    let id2 = make_id(1, ident_str!("Name2"));
    let id3 = make_id(2, ident_str!("Name3"));

    let mut sub_map = BTreeMap::new();
    sub_map.insert(id1.clone(), id1.clone());
    // reflexive sub should fail
    assert!(ModuleHandleRewriter::new(sub_map).is_err());

    // domain/range overlap should fail
    let mut sub_map = BTreeMap::new();
    sub_map.insert(id1, id2.clone());
    sub_map.insert(id2, id3);
    assert!(ModuleHandleRewriter::new(sub_map).is_err());
}

// the domain of sub_map must be present in the module we're trying to rewrite
#[test]
fn sub_target_does_not_exist() {
    let id1 = make_id(0, ident_str!("Name1"));
    let id2 = make_id(1, ident_str!("Name2"));

    let rewriter = {
        let mut sub_map = BTreeMap::new();
        sub_map.insert(id1, id2);
        ModuleHandleRewriter::new(sub_map).unwrap()
    };

    let mut m = file_format::empty_module();
    // fail's because there's no handle for `id1` in `m`
    assert!(rewriter.sub_module_ids(&mut m).is_err());
}

// the domain of sub_map must be present in the module we're trying to rewrite
#[test]
fn sub_friend_only() {
    let id1 = make_id(0, ident_str!("Name1"));
    let id2 = make_id(1, ident_str!("Name2"));

    let mut m = file_format::empty_module();
    let handle = make_module_handle(&id1, &mut m);
    m.friend_decls.push(handle);

    let rewriter = {
        let mut sub_map = BTreeMap::new();
        sub_map.insert(id1.clone(), id2.clone());
        ModuleHandleRewriter::new(sub_map).unwrap()
    };
    rewriter.sub_module_ids(&mut m).unwrap();

    assert!(m.address_identifier_at(m.friend_decls[0].address) == id2.address());
    assert!(m.identifier_at(m.friend_decls[0].name) == id2.name());
}

// Substitution between two module ID's that already exist in the module table
#[test]
fn sub_existing() {
    let id1 = make_id(10, ident_str!("Name1"));
    let id2 = make_id(11, ident_str!("Name2"));
    let id3 = make_id(12, ident_str!("Name3"));

    let mut m = file_format::empty_module();
    let idx1 = ModuleHandleRewriter::get_or_create_module_handle(&id1, &mut m);
    let idx2 = ModuleHandleRewriter::get_or_create_module_handle(&id2, &mut m);
    let idx3 = ModuleHandleRewriter::get_or_create_module_handle(&id3, &mut m);

    // set the self ID to idx1
    m.self_module_handle_idx = idx1;
    // add a struct, function handles, and friend that use idx1
    m.struct_handles.push(make_struct_handle(idx1));
    m.function_handles.push(make_function_handle(idx1));
    let friend_handle1 = make_module_handle(&id1, &mut m);
    m.friend_decls.push(friend_handle1);
    // add a struct and function handles that do not use idx1
    m.struct_handles.push(make_struct_handle(idx3));
    m.function_handles.push(make_function_handle(idx3));
    let friend_handle2 = make_module_handle(&id3, &mut m);
    m.friend_decls.push(friend_handle2);

    // substitute id1 for id2
    let old_handles = m.module_handles.clone();
    let rewriter = {
        let mut sub_map = BTreeMap::new();
        sub_map.insert(id1.clone(), id2.clone());
        ModuleHandleRewriter::new(sub_map).unwrap()
    };
    rewriter.sub_module_ids(&mut m).unwrap();
    // substitution should not change the module handles table if id2 is already in the module
    assert!(old_handles == m.module_handles);
    // substitution should change the contents of the id1 self address, struct handles, friends, and module handles to id2
    assert!(m.self_module_handle_idx == idx2);
    assert!(m.struct_handles[0].module == idx2);
    assert!(m.function_handles[0].module == idx2);
    assert!(m.address_identifier_at(m.friend_decls[0].address) == id2.address());
    assert!(m.identifier_at(m.friend_decls[0].name) == id2.name());
    // but should not change unrelated handles
    assert!(m.struct_handles[1].module == idx3);
    assert!(m.function_handles[1].module == idx3);
    assert!(m.address_identifier_at(m.friend_decls[1].address) == id3.address());
    assert!(m.identifier_at(m.friend_decls[1].name) == id3.name());
}

// substitution where the new ID does not yet exist in the module table
#[test]
fn sub_non_existing() {
    // id's that exist in the module
    let old_id1 = make_id(10, ident_str!("Name1"));
    let old_id2 = make_id(11, ident_str!("Name2"));
    // an id that does not exist in the module
    let new_id = make_id(12, ident_str!("Name3"));

    let mut m = file_format::empty_module();
    // add the old id's to the module
    let old_idx1 = ModuleHandleRewriter::get_or_create_module_handle(&old_id1, &mut m);
    let old_idx2 = ModuleHandleRewriter::get_or_create_module_handle(&old_id2, &mut m);

    // add some struct and function handles that use the old id's
    m.self_module_handle_idx = old_idx2;
    m.struct_handles.push(make_struct_handle(old_idx2));
    m.function_handles.push(make_function_handle(old_idx2));
    m.struct_handles.push(make_struct_handle(old_idx1));
    m.function_handles.push(make_function_handle(old_idx1));
    let friend_handle1 = make_module_handle(&old_id2, &mut m);
    let friend_handle2 = make_module_handle(&old_id1, &mut m);
    m.friend_decls.push(friend_handle1);
    m.friend_decls.push(friend_handle2);

    // substitute for new_id, which has not yet been added to the module
    let old_handles_len = m.module_handles.len();
    let rewriter = {
        let mut sub_map = BTreeMap::new();
        sub_map.insert(old_id1.clone(), new_id.clone());
        ModuleHandleRewriter::new(sub_map).unwrap()
    };
    rewriter.sub_module_ids(&mut m).unwrap();
    // module handles table should have grown to accomodate id2
    assert!(m.module_handles.len() == old_handles_len + 1);
    // new id should have been added to the end of the table
    let new_idx = ModuleHandleIndex((m.module_handles.len() - 1) as u16);
    assert!(m.module_id_for_handle(m.module_handle_at(new_idx)) == new_id);

    // substitution should change the contents of the id1 struct handles, and module handles
    assert!(m.struct_handles[1].module == new_idx);
    assert!(m.function_handles[1].module == new_idx);
    assert!(m.address_identifier_at(m.friend_decls[1].address) == new_id.address());
    assert!(m.identifier_at(m.friend_decls[1].name) == new_id.name());
    // but should not change unrelated handles
    assert!(m.self_module_handle_idx == old_idx2);
    assert!(m.struct_handles[0].module == old_idx2);
    assert!(m.function_handles[0].module == old_idx2);
    assert!(m.address_identifier_at(m.friend_decls[0].address) == old_id2.address());
    assert!(m.identifier_at(m.friend_decls[0].name) == old_id2.name());
}
