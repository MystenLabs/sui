// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::{
    Bytecode::*, CompiledModule, SignatureToken::*, Visibility::Public, *,
};
use move_bytecode_verifier::{
    verify_module_unmetered, verify_module_with_config_for_test, SignatureChecker,
};
use move_bytecode_verifier_meter::dummy::DummyMeter;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};

#[test]
fn test_reference_of_reference() {
    let mut m = basic_test_module();
    m.signatures[0] = Signature(vec![Reference(Box::new(Reference(Box::new(
        SignatureToken::Bool,
    ))))]);
    let errors = SignatureChecker::verify_module(&m);
    assert!(errors.is_err());
}

#[test]
fn no_verify_locals_good() {
    let compiled_module_good = CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        self_module_handle_idx: ModuleHandleIndex(0),
        datatype_handles: vec![],
        signatures: vec![
            Signature(vec![Address]),
            Signature(vec![U64]),
            Signature(vec![]),
        ],
        function_handles: vec![
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                return_: SignatureIndex(2),
                parameters: SignatureIndex(0),
                type_parameters: vec![],
            },
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(2),
                return_: SignatureIndex(2),
                parameters: SignatureIndex(1),
                type_parameters: vec![],
            },
        ],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        identifiers: vec![
            Identifier::new("Bad").unwrap(),
            Identifier::new("blah").unwrap(),
            Identifier::new("foo").unwrap(),
        ],
        address_identifiers: vec![AccountAddress::new([0; AccountAddress::LENGTH])],
        constant_pool: vec![],
        metadata: vec![],
        struct_defs: vec![],
        function_defs: vec![
            FunctionDefinition {
                function: FunctionHandleIndex(0),
                visibility: Visibility::Public,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![Ret],
                    jump_tables: vec![],
                }),
            },
            FunctionDefinition {
                function: FunctionHandleIndex(1),
                visibility: Visibility::Public,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(1),
                    code: vec![Ret],
                    jump_tables: vec![],
                }),
            },
        ],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    };
    assert!(verify_module_unmetered(&compiled_module_good).is_ok());
}

#[test]
fn big_signature_test() {
    const N_TYPE_PARAMS: usize = 5;
    const INSTANTIATION_DEPTH: usize = 3;
    const VECTOR_DEPTH: usize = 250;
    let mut st = SignatureToken::U8;
    for _ in 0..VECTOR_DEPTH {
        st = SignatureToken::Vector(Box::new(st));
    }
    for _ in 0..INSTANTIATION_DEPTH {
        let type_params = vec![st; N_TYPE_PARAMS];
        st = SignatureToken::DatatypeInstantiation(Box::new((DatatypeHandleIndex(0), type_params)));
    }

    const N_READPOP: u16 = 7500;

    let mut code = vec![];
    // 1. ImmBorrowLoc: ... ref
    // 2. ReadRef:      ... value
    // 3. Pop:          ...
    for _ in 0..N_READPOP {
        code.push(Bytecode::ImmBorrowLoc(0));
        code.push(Bytecode::ReadRef);
        code.push(Bytecode::Pop);
    }
    code.push(Bytecode::Ret);

    let type_param_constraints = DatatypeTyParameter {
        constraints: AbilitySet::EMPTY,
        is_phantom: false,
    };

    let module = CompiledModule {
        version: 5,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        datatype_handles: vec![DatatypeHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(1),
            abilities: AbilitySet::ALL,
            type_parameters: vec![type_param_constraints; N_TYPE_PARAMS],
        }],
        function_handles: vec![FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: SignatureIndex(1),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        }],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![Signature(vec![]), Signature(vec![st])],
        identifiers: vec![
            Identifier::new("f").unwrap(),
            Identifier::new("generic_struct").unwrap(),
        ],
        address_identifiers: vec![AccountAddress::ONE],
        constant_pool: vec![],
        metadata: vec![],
        struct_defs: vec![StructDefinition {
            struct_handle: DatatypeHandleIndex(0),
            field_information: StructFieldInformation::Native,
        }],
        function_defs: vec![FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility: Public,
            is_entry: true,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code,
                jump_tables: vec![],
            }),
        }],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    };

    // save module and verify that it can ser/de
    let mut mvbytes = vec![];
    module.serialize(&mut mvbytes).unwrap();
    let module = CompiledModule::deserialize_with_defaults(&mvbytes).unwrap();

    let res = verify_module_with_config_for_test(
        "big_signature_test",
        &production_config().0,
        &module,
        &mut DummyMeter,
    )
    .unwrap_err();
    assert_eq!(res.major_status(), StatusCode::TOO_MANY_TYPE_NODES);
}
