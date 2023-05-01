// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::unit_tests::production_config;
use move_binary_format::file_format::*;
use move_bytecode_verifier::{
    limits::LimitsVerifier, verifier::DEFAULT_MAX_IDENTIFIER_LENGTH,
    verify_module_with_config_for_test, VerifierConfig,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};

#[test]
fn test_function_handle_type_instantiation() {
    let mut m = basic_test_module();
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: std::iter::repeat(AbilitySet::ALL).take(10).collect(),
    });

    assert_eq!(
        LimitsVerifier::verify_module(
            &VerifierConfig {
                max_generic_instantiation_length: Some(9),
                ..Default::default()
            },
            &m
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_TYPE_PARAMETERS
    );

    let mut s = basic_test_script();
    s.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: std::iter::repeat(AbilitySet::ALL).take(10).collect(),
    });

    assert_eq!(
        LimitsVerifier::verify_script(
            &VerifierConfig {
                max_generic_instantiation_length: Some(9),
                ..Default::default()
            },
            &s
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_TYPE_PARAMETERS
    );
}

#[test]
fn test_struct_handle_type_instantiation() {
    let mut m = basic_test_module();
    m.struct_handles.push(StructHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        abilities: AbilitySet::ALL,
        type_parameters: std::iter::repeat(StructTypeParameter {
            constraints: AbilitySet::ALL,
            is_phantom: false,
        })
        .take(10)
        .collect(),
    });

    assert_eq!(
        LimitsVerifier::verify_module(
            &VerifierConfig {
                max_generic_instantiation_length: Some(9),
                ..Default::default()
            },
            &m
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_TYPE_PARAMETERS
    );

    let mut s = basic_test_script();
    s.struct_handles.push(StructHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        abilities: AbilitySet::ALL,
        type_parameters: std::iter::repeat(StructTypeParameter {
            constraints: AbilitySet::ALL,
            is_phantom: false,
        })
        .take(10)
        .collect(),
    });

    assert_eq!(
        LimitsVerifier::verify_script(
            &VerifierConfig {
                max_generic_instantiation_length: Some(9),
                ..Default::default()
            },
            &s
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_TYPE_PARAMETERS
    );
}

#[test]
fn test_function_handle_parameters() {
    let mut m = basic_test_module();
    m.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Bool).take(10).collect(),
    ));
    m.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });

    assert_eq!(
        LimitsVerifier::verify_module(
            &VerifierConfig {
                max_function_parameters: Some(9),
                ..Default::default()
            },
            &m
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_PARAMETERS
    );

    let mut s = basic_test_script();
    s.signatures.push(Signature(
        std::iter::repeat(SignatureToken::Bool).take(10).collect(),
    ));
    s.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex::new(0),
        name: IdentifierIndex::new(0),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });

    assert_eq!(
        LimitsVerifier::verify_script(
            &VerifierConfig {
                max_function_parameters: Some(9),
                ..Default::default()
            },
            &s
        )
        .unwrap_err()
        .major_status(),
        StatusCode::TOO_MANY_PARAMETERS
    );
}

#[test]
fn big_vec_unpacks() {
    const N_TYPE_PARAMS: usize = 16;
    let mut st = SignatureToken::Vector(Box::new(SignatureToken::U8));
    let type_params = vec![st; N_TYPE_PARAMS];
    st = SignatureToken::StructInstantiation(StructHandleIndex(0), type_params);
    const N_VEC_PUSH: u16 = 1000;
    let mut code = vec![];
    // 1. CopyLoc:     ...         -> ... st
    // 2. VecPack:     ... st      -> ... Vec<st>
    // 3. VecUnpack:   ... Vec<st> -> ... st, st, st, ... st
    for _ in 0..N_VEC_PUSH {
        code.push(Bytecode::CopyLoc(0));
        code.push(Bytecode::VecPack(SignatureIndex(1), 1));
        code.push(Bytecode::VecUnpack(SignatureIndex(1), 1 << 15));
    }
    // 1. VecPack:   ... st, st, st, ... st -> ... Vec<st>
    // 2. Pop:       ... Vec<st>            -> ...
    for _ in 0..N_VEC_PUSH {
        code.push(Bytecode::VecPack(SignatureIndex(1), 1 << 15));
        code.push(Bytecode::Pop);
    }
    code.push(Bytecode::Ret);
    let type_param_constraints = StructTypeParameter {
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
        struct_handles: vec![StructHandle {
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
            struct_handle: StructHandleIndex(0),
            field_information: StructFieldInformation::Native,
        }],
        function_defs: vec![FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility: Visibility::Public,
            is_entry: true,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code,
            }),
        }],
    };

    // save module and verify that it can ser/de
    let mut mvbytes = vec![];
    module.serialize(&mut mvbytes).unwrap();
    let module = CompiledModule::deserialize_with_defaults(&mvbytes).unwrap();

    let res = verify_module_with_config_for_test(
        "big_vec_unpacks",
        &VerifierConfig {
            max_loop_depth: Some(5),
            max_generic_instantiation_length: Some(32),
            max_function_parameters: Some(128),
            max_basic_blocks: Some(1024),
            max_push_size: Some(10000),
            ..Default::default()
        },
        &module,
    );
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::VALUE_STACK_PUSH_OVERFLOW
    );
}

const MAX_STRUCTS: usize = 200;
const MAX_FIELDS: usize = 30;
const MAX_FUNCTIONS: usize = 1000;

#[test]
fn max_struct_test() {
    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_fields_in_struct: Some(MAX_FIELDS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, 0);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    multi_struct(&mut module, 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS / 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_STRUCT_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_STRUCT_DEFINITIONS_REACHED,
    );
}

#[test]
fn max_fields_test() {
    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_fields_in_struct: Some(MAX_FIELDS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, 1);
    multi_fields(&mut module, MAX_FIELDS / 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 10);
    multi_fields(&mut module, MAX_FIELDS - 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 50);
    multi_fields(&mut module, MAX_FIELDS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 100);
    multi_fields(&mut module, MAX_FIELDS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, 2);
    multi_fields(&mut module, MAX_FIELDS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, 50);
    multi_fields_except_one(&mut module, 0, 2, MAX_FIELDS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, 20);
    multi_fields_except_one(&mut module, 19, MAX_FIELDS, MAX_FIELDS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, 100);
    multi_fields_except_one(&mut module, 50, 1, MAX_FIELDS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
}

#[test]
fn max_functions_test() {
    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_fields_in_struct: Some(MAX_FIELDS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, 1);
    multi_functions(&mut module, 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 10);
    multi_functions(&mut module, MAX_FUNCTIONS / 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 5);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 5);
    multi_functions(&mut module, MAX_FUNCTIONS - 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, 5);
    multi_functions(&mut module, MAX_FUNCTIONS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_functions(&mut module, MAX_FUNCTIONS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
    );
}

#[test]
fn max_mixed_config_test() {
    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_fields_in_struct: Some(MAX_FIELDS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));

    let config = VerifierConfig {
        max_function_definitions: None,
        max_struct_definitions: None,
        max_fields_in_struct: None,
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, 1);
    multi_fields(&mut module, 1);
    multi_functions(&mut module, 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 2);
    multi_fields(&mut module, MAX_FIELDS * 2);
    multi_functions(&mut module, MAX_FUNCTIONS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS + 1);
    multi_fields(&mut module, MAX_FIELDS + 1);
    multi_functions(&mut module, MAX_FUNCTIONS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));

    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_fields_in_struct: Some(MAX_FIELDS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS + 10);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS * 3);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 2);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS + 1);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_STRUCT_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS * 2);
    multi_functions(&mut module, MAX_FUNCTIONS * 3);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );

    let config = VerifierConfig {
        max_struct_definitions: Some(MAX_STRUCTS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS + 1);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS * 3);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 2);
    multi_fields(&mut module, MAX_FIELDS * 3);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_STRUCT_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS * 2);
    multi_functions(&mut module, MAX_FUNCTIONS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
    );

    let config = VerifierConfig {
        max_fields_in_struct: Some(MAX_FIELDS),
        max_function_definitions: Some(MAX_FUNCTIONS),
        ..Default::default()
    };
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 3);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS + 1);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(res, Ok(()));
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS + 1);
    multi_fields(&mut module, MAX_FIELDS * 3);
    multi_functions(&mut module, MAX_FUNCTIONS);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FIELD_DEFINITIONS_REACHED,
    );
    let mut module = leaf_module("M");
    multi_struct(&mut module, MAX_STRUCTS * 2);
    multi_fields(&mut module, MAX_FIELDS);
    multi_functions(&mut module, MAX_FUNCTIONS * 2);
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::MAX_FUNCTION_DEFINITIONS_REACHED,
    );
}

#[test]
fn max_identifier_len() {
    let config = production_config();
    let max_ident = "z".repeat(
        config
            .max_idenfitier_len
            .unwrap_or(DEFAULT_MAX_IDENTIFIER_LENGTH) as usize,
    );
    let good_module = leaf_module(&max_ident);

    let res = LimitsVerifier::verify_module(&config, &good_module);
    assert!(res.is_ok());

    let config = production_config();
    let max_ident = "z".repeat(
        (config
            .max_idenfitier_len
            .unwrap_or(DEFAULT_MAX_IDENTIFIER_LENGTH) as usize)
            / 2,
    );
    let good_module = leaf_module(&max_ident);

    let res = LimitsVerifier::verify_module(&config, &good_module);
    assert!(res.is_ok());

    let over_max_ident = "z".repeat(
        1 + config
            .max_idenfitier_len
            .unwrap_or(DEFAULT_MAX_IDENTIFIER_LENGTH) as usize,
    );
    let bad_module = leaf_module(&over_max_ident);
    let res = LimitsVerifier::verify_module(&config, &bad_module);

    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::IDENTIFIER_TOO_LONG,
    );

    let over_max_ident = "zx".repeat(
        1 + config
            .max_idenfitier_len
            .unwrap_or(DEFAULT_MAX_IDENTIFIER_LENGTH) as usize,
    );
    let bad_module = leaf_module(&over_max_ident);
    let res = LimitsVerifier::verify_module(&config, &bad_module);

    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::IDENTIFIER_TOO_LONG,
    );
}

#[test]
fn max_vec_len() {
    let config = VerifierConfig {
        max_constant_vector_len: Some(0xFFFF - 1),
        ..Default::default()
    };
    let double_vec = |item: Vec<u8>| -> Vec<u8> {
        let mut items = vec![2];
        items.extend(item.clone());
        items.extend(item);
        items
    };
    let large_vec = |item: Vec<u8>| -> Vec<u8> {
        let mut items = vec![0xFF, 0xFF, 3];
        (0..0xFFFF).for_each(|_| items.extend(item.clone()));
        items
    };
    fn tvec(s: SignatureToken) -> SignatureToken {
        SignatureToken::Vector(Box::new(s))
    }

    let mut module = empty_module();
    module.constant_pool = vec![Constant {
        type_: tvec(SignatureToken::Bool),
        data: large_vec(vec![0]),
    }];
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::TOO_MANY_VECTOR_ELEMENTS,
    );

    let mut module = empty_module();
    module.constant_pool = vec![Constant {
        type_: tvec(SignatureToken::U256),
        data: large_vec(vec![
            0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            0, 0, 0,
        ]),
    }];
    let res = LimitsVerifier::verify_module(&config, &module);
    assert_eq!(
        res.unwrap_err().major_status(),
        StatusCode::TOO_MANY_VECTOR_ELEMENTS,
    );

    let config = VerifierConfig {
        max_constant_vector_len: Some(0xFFFF),
        ..Default::default()
    };

    let mut module = empty_module();
    module.constant_pool = vec![
        // empty
        Constant {
            type_: tvec(SignatureToken::Bool),
            data: vec![0],
        },
        Constant {
            type_: tvec(tvec(SignatureToken::Bool)),
            data: vec![0],
        },
        Constant {
            type_: tvec(tvec(tvec(tvec(SignatureToken::Bool)))),
            data: vec![0],
        },
        Constant {
            type_: tvec(tvec(tvec(tvec(SignatureToken::Bool)))),
            data: double_vec(vec![0]),
        },
        // small
        Constant {
            type_: tvec(SignatureToken::Bool),
            data: vec![9, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        },
        Constant {
            type_: tvec(SignatureToken::U8),
            data: vec![9, 1, 1, 1, 1, 1, 1, 1, 1, 1],
        },
        // large
        Constant {
            type_: tvec(SignatureToken::Bool),
            data: large_vec(vec![0]),
        },
        Constant {
            type_: tvec(SignatureToken::U8),
            data: large_vec(vec![0]),
        },
        Constant {
            type_: tvec(SignatureToken::U16),
            data: large_vec(vec![0, 0]),
        },
        Constant {
            type_: tvec(SignatureToken::U32),
            data: large_vec(vec![0, 0, 0, 0]),
        },
        Constant {
            type_: tvec(SignatureToken::U64),
            data: large_vec(vec![0, 0, 0, 0, 0, 0, 0, 0]),
        },
        Constant {
            type_: tvec(SignatureToken::U128),
            data: large_vec(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        },
        Constant {
            type_: tvec(SignatureToken::U256),
            data: large_vec(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ]),
        },
        Constant {
            type_: tvec(SignatureToken::Address),
            data: large_vec(vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
        },
        // double large
        Constant {
            type_: tvec(tvec(SignatureToken::Bool)),
            data: double_vec(large_vec(vec![0])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U8)),
            data: double_vec(large_vec(vec![0])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U16)),
            data: double_vec(large_vec(vec![0, 0])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U32)),
            data: double_vec(large_vec(vec![0, 0, 0, 0])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U64)),
            data: double_vec(large_vec(vec![0, 0, 0, 0, 0, 0, 0, 0])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U128)),
            data: double_vec(large_vec(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::U256)),
            data: double_vec(large_vec(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
                0, 0, 0, 0,
            ])),
        },
        Constant {
            type_: tvec(tvec(SignatureToken::Address)),
            data: double_vec(large_vec(vec![
                0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
            ])),
        },
    ];
    let res = LimitsVerifier::verify_module(&config, &module);

    assert!(res.is_ok());
}

fn multi_struct(module: &mut CompiledModule, count: usize) {
    for i in 0..count {
        module
            .identifiers
            .push(Identifier::new(format!("A_{}", i)).unwrap());
        module.struct_handles.push(StructHandle {
            module: module.self_module_handle_idx,
            name: IdentifierIndex((module.identifiers.len() - 1) as u16),
            abilities: AbilitySet::EMPTY,
            type_parameters: vec![],
        });
        module.struct_defs.push(StructDefinition {
            struct_handle: StructHandleIndex((module.struct_handles.len() - 1) as u16),
            field_information: StructFieldInformation::Declared(vec![]),
        });
    }
}

fn multi_fields(module: &mut CompiledModule, count: usize) {
    for def in &mut module.struct_defs {
        let mut fields = vec![];
        for i in 0..count {
            module
                .identifiers
                .push(Identifier::new(format!("f_{}", i)).unwrap());
            fields.push(FieldDefinition {
                name: Default::default(),
                signature: TypeSignature(SignatureToken::U8),
            });
        }
        def.field_information = StructFieldInformation::Declared(fields);
    }
}

fn multi_fields_except_one(module: &mut CompiledModule, idx: usize, count: usize, one: usize) {
    for (struct_idx, def) in module.struct_defs.iter_mut().enumerate() {
        let mut fields = vec![];
        let count = if struct_idx == idx { one } else { count };
        for i in 0..count {
            module
                .identifiers
                .push(Identifier::new(format!("f_{}", i)).unwrap());
            fields.push(FieldDefinition {
                name: Default::default(),
                signature: TypeSignature(SignatureToken::U8),
            });
        }
        def.field_information = StructFieldInformation::Declared(fields);
    }
}

fn multi_functions(module: &mut CompiledModule, count: usize) {
    module.signatures.push(Signature(vec![]));
    for i in 0..count {
        module
            .identifiers
            .push(Identifier::new(format!("func_{}", i)).unwrap());
        module.function_handles.push(FunctionHandle {
            module: module.self_module_handle_idx,
            name: IdentifierIndex((module.identifiers.len() - 1) as u16),
            parameters: SignatureIndex((module.signatures.len() - 1) as u16),
            return_: SignatureIndex((module.signatures.len() - 1) as u16),
            type_parameters: vec![],
        });
        module.function_defs.push(FunctionDefinition {
            function: FunctionHandleIndex((module.function_handles.len() - 1) as u16),
            visibility: Visibility::Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex((module.signatures.len() - 1) as u16),
                code: vec![Bytecode::Ret],
            }),
        });
    }
}

fn leaf_module(name: &str) -> CompiledModule {
    let mut module = empty_module();
    module.identifiers[0] = Identifier::new(name).unwrap();
    module.address_identifiers[0] = AccountAddress::ONE;
    module
}
