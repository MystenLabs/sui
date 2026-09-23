// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    CompiledModule,
    file_format::{
        AddressIdentifierIndex, FunctionDefinition, FunctionHandle, FunctionHandleIndex,
        IdentifierIndex, ModuleHandle, ModuleHandleIndex, SignatureIndex, Visibility, empty_module,
    },
};
use move_bytecode_verifier::dependencies::{DependencyIndex, IndexedModule, verify_module};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};
use std::rc::Rc;

const ADDRESS: AccountAddress = AccountAddress::ONE;

#[test]
fn dependency_index_reuses_caller_specific_visibility_for_entry_functions() {
    let called_module = called_module();
    let dependencies = [&called_module];
    let dependency_index = DependencyIndex::new(dependencies);
    let indexed_module = Rc::new(IndexedModule::new(&called_module));
    let indexed_dependency_index =
        DependencyIndex::from_indexed_modules([Rc::clone(&indexed_module)]);

    assert_verification_result(
        calling_module("PublicCaller", "public_function"),
        &dependencies,
        &dependency_index,
        &indexed_dependency_index,
        None,
    );
    assert_verification_result(
        calling_module("Friend", "friend_function"),
        &dependencies,
        &dependency_index,
        &indexed_dependency_index,
        None,
    );
    assert_verification_result(
        calling_module("UnauthorizedCaller", "friend_function"),
        &dependencies,
        &dependency_index,
        &indexed_dependency_index,
        Some(StatusCode::LOOKUP_FAILED),
    );
    assert_verification_result(
        calling_module("PrivateCaller", "private_function"),
        &dependencies,
        &dependency_index,
        &indexed_dependency_index,
        Some(StatusCode::LOOKUP_FAILED),
    );
}

fn assert_verification_result(
    calling_module: CompiledModule,
    dependencies: &[&CompiledModule],
    dependency_index: &DependencyIndex<'_>,
    indexed_dependency_index: &DependencyIndex<'_>,
    expected_status: Option<StatusCode>,
) {
    let wrapper_result = verify_module(
        &DependencyIndex::new(dependencies.iter().copied()),
        &calling_module,
    );
    let indexed_result = verify_module(dependency_index, &calling_module);
    let indexed_modules_result = verify_module(indexed_dependency_index, &calling_module);
    assert_eq!(wrapper_result, indexed_result);
    assert_eq!(wrapper_result, indexed_modules_result);

    match expected_status {
        None => indexed_result.unwrap(),
        Some(status) => assert_eq!(indexed_result.unwrap_err().major_status(), status),
    }
}

fn called_module() -> CompiledModule {
    let mut module = named_module("Called");

    let friend_name = add_identifier(&mut module, "Friend");
    let friend_handle = ModuleHandle {
        address: AddressIdentifierIndex(0),
        name: friend_name,
    };
    module.module_handles.push(friend_handle.clone());
    module.friend_decls.push(friend_handle);

    add_function(&mut module, "public_function", Visibility::Public);
    add_function(&mut module, "friend_function", Visibility::Friend);
    add_function(&mut module, "private_function", Visibility::Private);
    module
}

fn calling_module(name: &str, called_function: &str) -> CompiledModule {
    let mut module = named_module(name);

    let called_module_name = add_identifier(&mut module, "Called");
    module.module_handles.push(ModuleHandle {
        address: AddressIdentifierIndex(0),
        name: called_module_name,
    });
    let function_name = add_identifier(&mut module, called_function);
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(1),
        name: function_name,
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    module
}

fn named_module(name: &str) -> CompiledModule {
    let mut module = empty_module();
    module.address_identifiers[0] = ADDRESS;
    module.identifiers[0] = Identifier::new(name).unwrap();
    module
}

fn add_identifier(module: &mut CompiledModule, name: &str) -> IdentifierIndex {
    let index = IdentifierIndex(module.identifiers.len() as u16);
    module.identifiers.push(Identifier::new(name).unwrap());
    index
}

fn add_function(module: &mut CompiledModule, name: &str, visibility: Visibility) {
    let handle = FunctionHandleIndex(module.function_handles.len() as u16);
    let function_name = add_identifier(module, name);
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: function_name,
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: handle,
        visibility,
        is_entry: true,
        acquires_global_resources: vec![],
        code: None,
    });
}
