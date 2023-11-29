// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::*;
use move_bytecode_verifier::dependencies;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};

fn mk_script_function_module() -> CompiledModule {
    let m = CompiledModule {
        version: move_binary_format::file_format_common::VERSION_4,
        module_handles: vec![
            // only self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![
            Identifier::new("M").unwrap(),    // Module name
            Identifier::new("fn").unwrap(),   // Function name
            Identifier::new("g_fn").unwrap(), // Generic function name
        ],
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        function_handles: vec![
            // fun fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
            // fun g_fn<T>()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(2),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![AbilitySet::EMPTY],
            },
        ],
        function_defs: vec![
            // public(script)  fun fn() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(0),
                visibility: Visibility::Public,
                is_entry: true,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![Bytecode::Ret],
                    jump_tables: vec![],
                }),
            },
            // public(script) fun g_fn<T>() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(1),
                visibility: Visibility::Public,
                is_entry: true,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![Bytecode::Ret],
                    jump_tables: vec![],
                }),
            },
        ],
        signatures: vec![
            Signature(vec![]), // void
        ],
        struct_defs: vec![],
        datatype_handles: vec![],
        constant_pool: vec![],
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
    };
    move_bytecode_verifier::verify_module_unmetered(&m).unwrap();
    m
}

fn mk_invoking_module(use_generic: bool, valid: bool) -> CompiledModule {
    let call = if use_generic {
        Bytecode::CallGeneric(FunctionInstantiationIndex(0))
    } else {
        Bytecode::Call(FunctionHandleIndex(1))
    };
    let m = CompiledModule {
        version: move_binary_format::file_format_common::VERSION_4,
        module_handles: vec![
            // self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
            // other module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(2),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![
            Identifier::new("Test").unwrap(),    // Module name
            Identifier::new("test_fn").unwrap(), // test name
            Identifier::new("M").unwrap(),       // Other Module name
            Identifier::new("fn").unwrap(),      // Other Function name
            Identifier::new("g_fn").unwrap(),    // Other Generic function name
        ],
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        function_handles: vec![
            // Self::test_fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
            // 0::M::fn()
            FunctionHandle {
                module: ModuleHandleIndex(1),
                name: IdentifierIndex(3),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
            // 0::M::g_fn<T>()
            FunctionHandle {
                module: ModuleHandleIndex(1),
                name: IdentifierIndex(4),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![AbilitySet::EMPTY],
            },
        ],
        // 0::M::g_fn<u64>()
        function_instantiations: vec![FunctionInstantiation {
            handle: FunctionHandleIndex(2),
            type_parameters: SignatureIndex(1),
        }],
        function_defs: vec![
            // fun fn() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(0),
                visibility: Visibility::Public,
                is_entry: valid,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![call, Bytecode::Ret],
                    jump_tables: vec![],
                }),
            },
        ],
        signatures: vec![
            Signature(vec![]),                    // void
            Signature(vec![SignatureToken::U64]), // u64
        ],
        struct_defs: vec![],
        datatype_handles: vec![],
        constant_pool: vec![],
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        field_instantiations: vec![],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
    };
    move_bytecode_verifier::verify_module_unmetered(&m).unwrap();
    m
}

fn mk_invoking_script(use_generic: bool) -> CompiledScript {
    let call = if use_generic {
        Bytecode::CallGeneric(FunctionInstantiationIndex(0))
    } else {
        Bytecode::Call(FunctionHandleIndex(0))
    };
    let s = CompiledScript {
        version: move_binary_format::file_format_common::VERSION_4,
        module_handles: vec![
            // other module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
        ],
        identifiers: vec![
            Identifier::new("M").unwrap(),    // Other Module name
            Identifier::new("fn").unwrap(),   // Other Function name
            Identifier::new("g_fn").unwrap(), // Other Generic function name
        ],
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        function_handles: vec![
            // 0::M::fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
            // 0::M::g_fn<T>()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(2),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![AbilitySet::EMPTY],
            },
        ],
        // 0::M::g_fn<u64>()
        function_instantiations: vec![FunctionInstantiation {
            handle: FunctionHandleIndex(1),
            type_parameters: SignatureIndex(1),
        }],
        type_parameters: vec![],
        parameters: SignatureIndex(0),
        code: CodeUnit {
            locals: SignatureIndex(0),
            code: vec![call, Bytecode::Ret],
            jump_tables: vec![],
        },
        signatures: vec![
            Signature(vec![]),                    // void
            Signature(vec![SignatureToken::U64]), // u64
        ],
        datatype_handles: vec![],
        constant_pool: vec![],
        metadata: vec![],
    };
    move_bytecode_verifier::verify_script_unmetered(&s).unwrap();
    s
}

#[test]

// tests the deprecated Script visibility logic for < V5
// tests correct permissible invocation of Script functions
fn deprecated_script_visibility_checks_valid() {
    let script_function_module = mk_script_function_module();
    let deps = &[script_function_module];

    // module uses script functions from script context
    let is_valid = true;
    let non_generic_call_mod = mk_invoking_module(false, is_valid);
    let result = dependencies::verify_module(&non_generic_call_mod, deps);
    assert!(result.is_ok());

    let generic_call_mod = mk_invoking_module(true, is_valid);
    let result = dependencies::verify_module(&generic_call_mod, deps);
    assert!(result.is_ok());

    // script uses script functions
    let non_generic_call_script = mk_invoking_script(false);
    let result = dependencies::verify_script(&non_generic_call_script, deps);
    assert!(result.is_ok());

    let generic_call_script = mk_invoking_script(true);
    let result = dependencies::verify_script(&generic_call_script, deps);
    assert!(result.is_ok());
}

#[test]
// tests the deprecated Script visibility logic for < V5
// tests correct non-permissible invocation of Script functions
fn deprecated_script_visibility_checks_invalid() {
    let script_function_module = mk_script_function_module();
    let deps = &[script_function_module];

    // module uses script functions from script context
    let not_valid = false;
    let non_generic_call_mod = mk_invoking_module(false, not_valid);
    let result = dependencies::verify_module(&non_generic_call_mod, deps);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CALLED_SCRIPT_VISIBLE_FROM_NON_SCRIPT_VISIBLE,
    );

    let generic_call_mod = mk_invoking_module(true, not_valid);
    let result = dependencies::verify_module(&generic_call_mod, deps);
    assert_eq!(
        result.unwrap_err().major_status(),
        StatusCode::CALLED_SCRIPT_VISIBLE_FROM_NON_SCRIPT_VISIBLE,
    );
}
