// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    binary_config::{BinaryConfig, TableConfig},
    errors::BinaryLoaderResult,
    file_format::*,
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};
// Add simple proptest tests
// use proptest::prelude::*;

// Serialize a `CompiledModule` and deserialize with a given config (round trip)
fn deserialize(
    module: &CompiledModule,
    binary_config: &BinaryConfig,
) -> BinaryLoaderResult<CompiledModule> {
    // serialize the module
    let mut bytes = vec![];
    module.serialize(&mut bytes).unwrap();
    // deserialize the module just serialized (round trip)
    CompiledModule::deserialize_with_config(&bytes, binary_config)
}

//
// macro helpers
//

// check that the error is for limits on the specific table
macro_rules! assert_limit_failure {
    ($result:expr, $table:expr) => {
        assert!($result.is_err());
        let err = $result.unwrap_err();
        let (major_status, _, message, _, _, _) = err.all_data();
        assert_eq!(major_status, StatusCode::MALFORMED);
        assert!(message.as_ref().unwrap().contains($table));
    };
}

// check with values below, equal and above the given limit
macro_rules! check_limit {
    ($module:expr, $field:ident, $elem:expr, $config:expr, $table_kind:expr) => {
        let max_size = $config.table_config.$field;
        // under max ok
        $module.$field = vec![$elem.clone(); max_size as usize - 1];
        assert!(deserialize($module, $config).is_ok());
        // max ok
        $module.$field = vec![$elem.clone(); max_size as usize];
        assert!(deserialize($module, $config).is_ok());
        // over max error
        $module.$field = vec![$elem; max_size as usize + 1];
        assert_limit_failure!(deserialize($module, $config), $table_kind);
    };
}

#[test]
fn binary_limits_test() {
    let module = empty_module();
    let mut binary_config = BinaryConfig::standard();

    // legacy config should work
    let res = deserialize(&module, &binary_config);
    assert!(res.is_ok());

    // make given max size for different tables
    binary_config.table_config = TableConfig {
        module_handles: 10,
        datatype_handles: 15,
        function_handles: 11,
        function_instantiations: 5,
        signatures: 20,
        constant_pool: 12,
        identifiers: 10,
        address_identifiers: 8,
        struct_defs: 12,
        struct_def_instantiations: 13,
        function_defs: 9,
        field_handles: 11,
        field_instantiations: 16,
        friend_decls: 12,
        enum_defs: 12,
        enum_def_instantiations: 13,
        variant_handles: 11,
        variant_instantiation_handles: 16,
    };

    // From file_format_common.rs::TableType test table type of interest (no METADATA

    // MODULE_HANDLES
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        module_handles,
        ModuleHandle {
            address: AddressIdentifierIndex::new(0),
            name: IdentifierIndex::new(0)
        },
        &binary_config,
        "MODULE_HANDLES"
    );

    // DATATYPE_HANDLES
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        datatype_handles,
        DatatypeHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            abilities: AbilitySet::EMPTY,
            type_parameters: vec![],
        },
        &binary_config,
        "DATATYPE_HANDLES"
    );

    // FUNCTION_HANDLES
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        function_handles,
        FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        },
        &binary_config,
        "FUNCTION_HANDLES"
    );

    // FUNCTION_INST
    let module_test = &mut module.clone();
    module_test.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    check_limit!(
        module_test,
        function_instantiations,
        FunctionInstantiation {
            handle: FunctionHandleIndex(0),
            type_parameters: SignatureIndex(0),
        },
        &binary_config,
        "FUNCTION_INST"
    );

    // SIGNATURES
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        signatures,
        Signature(vec![]),
        &binary_config,
        "SIGNATURES"
    );

    // CONSTANT_POOL
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        constant_pool,
        Constant {
            type_: SignatureToken::U64,
            data: vec![0; 8],
        },
        &binary_config,
        "CONSTANT_POOL"
    );

    // IDENTIFIERS
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        identifiers,
        Identifier::new("ident").unwrap(),
        &binary_config,
        "IDENTIFIERS"
    );

    // ADDRESS_IDENTIFIERS
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        address_identifiers,
        AccountAddress::random(),
        &binary_config,
        "ADDRESS_IDENTIFIERS"
    );

    // STRUCT_DEFS
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    check_limit!(
        module_test,
        struct_defs,
        StructDefinition {
            struct_handle: DatatypeHandleIndex(0),
            field_information: StructFieldInformation::Declared(vec![]),
        },
        &binary_config,
        "STRUCT_DEFS"
    );

    // STRUCT_DEF_INST
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.struct_defs.push(StructDefinition {
        struct_handle: DatatypeHandleIndex(0),
        field_information: StructFieldInformation::Declared(vec![]),
    });
    check_limit!(
        module_test,
        struct_def_instantiations,
        StructDefInstantiation {
            def: StructDefinitionIndex(0),
            type_parameters: SignatureIndex(0),
        },
        &binary_config,
        "STRUCT_DEF_INST"
    );

    // ENUM_DEFS
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    check_limit!(
        module_test,
        enum_defs,
        EnumDefinition {
            enum_handle: DatatypeHandleIndex(0),
            variants: vec![VariantDefinition {
                variant_name: IdentifierIndex(0),
                fields: vec![],
            }]
        },
        &binary_config,
        "ENUM_DEFS"
    );

    // ENUM_DEF_INST
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.enum_defs.push(EnumDefinition {
        enum_handle: DatatypeHandleIndex(0),
        variants: vec![VariantDefinition {
            variant_name: IdentifierIndex(0),
            fields: vec![],
        }],
    });
    check_limit!(
        module_test,
        enum_def_instantiations,
        EnumDefInstantiation {
            def: EnumDefinitionIndex(0),
            type_parameters: SignatureIndex(0),
        },
        &binary_config,
        "ENUM_DEF_INST"
    );

    // FUNCTION_DEFS
    let module_test = &mut module.clone();
    module_test.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    check_limit!(
        module_test,
        function_defs,
        FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility: Visibility::Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![Bytecode::Ret],
                jump_tables: vec![],
            }),
        },
        &binary_config,
        "FUNCTION_DEFS"
    );

    // FIELD_HANDLE
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.struct_defs.push(StructDefinition {
        struct_handle: DatatypeHandleIndex(0),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(0),
            signature: TypeSignature(SignatureToken::Bool),
        }]),
    });
    check_limit!(
        module_test,
        field_handles,
        FieldHandle {
            owner: StructDefinitionIndex(0),
            field: 0,
        },
        &binary_config,
        "FIELD_HANDLE"
    );

    // FIELD_INST
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.struct_defs.push(StructDefinition {
        struct_handle: DatatypeHandleIndex(0),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(0),
            signature: TypeSignature(SignatureToken::Bool),
        }]),
    });
    module_test.field_handles.push(FieldHandle {
        owner: StructDefinitionIndex(0),
        field: 0,
    });
    check_limit!(
        module_test,
        field_instantiations,
        FieldInstantiation {
            handle: FieldHandleIndex(0),
            type_parameters: SignatureIndex(0),
        },
        &binary_config,
        "FIELD_INST"
    );

    // VARIANT_HANDLE
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.enum_defs.push(EnumDefinition {
        enum_handle: DatatypeHandleIndex(0),
        variants: vec![VariantDefinition {
            variant_name: IdentifierIndex(0),
            fields: vec![FieldDefinition {
                name: IdentifierIndex(0),
                signature: TypeSignature(SignatureToken::Bool),
            }],
        }],
    });
    check_limit!(
        module_test,
        variant_handles,
        VariantHandle {
            enum_def: EnumDefinitionIndex(0),
            variant: 0,
        },
        &binary_config,
        "VARIANT_HANDLE"
    );

    // VARIANT_INST
    let module_test = &mut module.clone();
    module_test.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(0),
        abilities: AbilitySet::EMPTY,
        type_parameters: vec![],
    });
    module_test.enum_defs.push(EnumDefinition {
        enum_handle: DatatypeHandleIndex(0),
        variants: vec![VariantDefinition {
            variant_name: IdentifierIndex(0),
            fields: vec![FieldDefinition {
                name: IdentifierIndex(0),
                signature: TypeSignature(SignatureToken::Bool),
            }],
        }],
    });
    module_test
        .enum_def_instantiations
        .push(EnumDefInstantiation {
            def: EnumDefinitionIndex(0),
            type_parameters: SignatureIndex(0),
        });
    check_limit!(
        module_test,
        variant_instantiation_handles,
        VariantInstantiationHandle {
            enum_def: EnumDefInstantiationIndex(0),
            variant: 0,
        },
        &binary_config,
        "VARIANT_INST"
    );

    // FRIEND_DECLS
    let module_test = &mut module.clone();
    check_limit!(
        module_test,
        friend_decls,
        ModuleHandle {
            address: AddressIdentifierIndex::new(0),
            name: IdentifierIndex::new(0)
        },
        &binary_config,
        "FRIEND_DECLS"
    );
}
