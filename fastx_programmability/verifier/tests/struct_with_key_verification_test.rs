use fastx_types::FASTX_FRAMEWORK_ADDRESS;
use fastx_verifier::struct_with_key_verifier::verify_module;

use move_binary_format::file_format::*;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

fn make_module() -> CompiledModule {
    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![
            // ID module from fastx framework.
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
            // Test module with same name ("ID") but at different address.
            ModuleHandle {
                address: AddressIdentifierIndex(1),
                name: IdentifierIndex(0),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(1),
        identifiers: vec![
            Identifier::new("ID").unwrap(), // ID Module name as well as struct name
            Identifier::new("foo").unwrap(), // Test struct name
            Identifier::new("id").unwrap(), // id field
        ],
        address_identifiers: vec![
            FASTX_FRAMEWORK_ADDRESS,                            // ID Module address
            AccountAddress::new([1u8; AccountAddress::LENGTH]), // A random address
        ],
        struct_handles: vec![
            // The FASTX_FRAMEWORK_ADDRESS::ID::ID struct
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(0),
                abilities: AbilitySet::EMPTY | Ability::Store | Ability::Drop,
                type_parameters: vec![],
            },
            // A random ID::ID struct from a different address
            StructHandle {
                module: ModuleHandleIndex(1),
                name: IdentifierIndex(0),
                abilities: AbilitySet::EMPTY | Ability::Store | Ability::Drop,
                type_parameters: vec![],
            },
        ],
        struct_defs: vec![],
        function_handles: vec![],
        function_defs: vec![],
        signatures: vec![
            Signature(vec![]),                       // void
            Signature(vec![SignatureToken::Signer]), // Signer
        ],
        constant_pool: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
    }
}

#[test]
fn key_struct_with_drop() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex(1),
        abilities: AbilitySet::EMPTY | Ability::Key | Ability::Drop,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(2),
            signature: TypeSignature(SignatureToken::U64),
        }]),
    });
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains("Struct foo cannot have both key and drop abilities"));
}

#[test]
fn non_key_struct_without_fields() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex(1),
        abilities: AbilitySet::EMPTY | Ability::Drop,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![]),
    });
    assert!(verify_module(&module).is_ok());
}

#[test]
fn key_struct_without_fields() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex(1),
        abilities: AbilitySet::EMPTY | Ability::Key,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![]),
    });
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains("First field of struct foo must be 'id', no field found"));
}

#[test]
fn key_struct_first_field_not_id() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex(1),
        abilities: AbilitySet::EMPTY | Ability::Key,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(1),
            signature: TypeSignature(SignatureToken::U64),
        }]),
    });
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains("First field of struct foo must be 'id', foo found"));

    module.struct_defs.pop();
    // Make id the second field, and it should still fail verification.
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![
            FieldDefinition {
                name: IdentifierIndex(1),
                signature: TypeSignature(SignatureToken::U64),
            },
            FieldDefinition {
                name: IdentifierIndex(2),
                signature: TypeSignature(SignatureToken::Struct(StructHandleIndex(0))),
            },
        ]),
    });
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains("First field of struct foo must be 'id', foo found"));
}

#[test]
fn key_struct_id_field_incorrect_type() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        abilities: AbilitySet::EMPTY | Ability::Key,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(2),
            signature: TypeSignature(SignatureToken::U64),
        }]),
    });
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains("First field of struct foo must be of ID type, U64 type found"));

    module.struct_defs.pop();
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(2),
            signature: TypeSignature(SignatureToken::Struct(StructHandleIndex(1))),
        }]),
    });
    assert!(verify_module(&module).unwrap_err().to_string().contains("First field of struct foo must be of type 00000000000000000000000000000002::ID::ID, 01010101010101010101010101010101::ID::ID type found"));

    module.struct_defs.pop();
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(2),
            signature: TypeSignature(SignatureToken::Struct(StructHandleIndex(2))),
        }]),
    });
    assert!(verify_module(&module).unwrap_err().to_string().contains("First field of struct foo must be of type 00000000000000000000000000000002::ID::ID, 00000000000000000000000000000002::ID::foo type found"));
}

#[test]
fn key_struct_id_field_valid() {
    let mut module = make_module();
    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(1),
        name: IdentifierIndex(2),
        abilities: AbilitySet::EMPTY | Ability::Key,
        type_parameters: vec![],
    });
    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(2),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex(2),
            signature: TypeSignature(SignatureToken::Struct(StructHandleIndex(0))),
        }]),
    });
    assert!(verify_module(&module).is_ok());
}
