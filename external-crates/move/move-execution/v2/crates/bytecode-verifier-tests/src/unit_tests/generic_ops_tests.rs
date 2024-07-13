// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::file_format::*;
use move_bytecode_verifier::InstructionConsistency;
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};

// Make a Module with 2 structs and 2 resources with one field each, and 2 functions.
// One of the struct/resource and one of the function is generic, the other "normal".
// Also make a test function whose body will be filled by given test cases.
fn make_module() -> CompiledModule {
    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![
            // only self module
            ModuleHandle {
                address: AddressIdentifierIndex(0),
                name: IdentifierIndex(0),
            },
        ],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![
            Identifier::new("M").unwrap(),       // Module name
            Identifier::new("S").unwrap(),       // Struct name
            Identifier::new("GS").unwrap(),      // Generic struct name
            Identifier::new("R").unwrap(),       // Resource name
            Identifier::new("GR").unwrap(),      // Generic resource name
            Identifier::new("f").unwrap(),       // Field name
            Identifier::new("fn").unwrap(),      // Function name
            Identifier::new("g_fn").unwrap(),    // Generic function name
            Identifier::new("test_fn").unwrap(), // Test function name
        ],
        address_identifiers: vec![
            AccountAddress::ZERO, // Module address
        ],
        datatype_handles: vec![
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                abilities: AbilitySet::PRIMITIVES,
                type_parameters: vec![],
            },
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(2),
                abilities: AbilitySet::PRIMITIVES,
                type_parameters: vec![DatatypeTyParameter {
                    constraints: AbilitySet::PRIMITIVES,
                    is_phantom: false,
                }],
            },
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(3),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![],
            },
            DatatypeHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(4),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![DatatypeTyParameter {
                    constraints: AbilitySet::PRIMITIVES,
                    is_phantom: false,
                }],
            },
        ],
        struct_defs: vec![
            // struct S { f: u64 }
            StructDefinition {
                struct_handle: DatatypeHandleIndex(0),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(5),
                    signature: TypeSignature(SignatureToken::U64),
                }]),
            },
            // struct GS<T> { f: T }
            StructDefinition {
                struct_handle: DatatypeHandleIndex(1),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(5),
                    signature: TypeSignature(SignatureToken::TypeParameter(0)),
                }]),
            },
            // struct R has key { f: u64 }
            StructDefinition {
                struct_handle: DatatypeHandleIndex(2),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(5),
                    signature: TypeSignature(SignatureToken::U64),
                }]),
            },
            // struct GR<T> has key { f: T }
            StructDefinition {
                struct_handle: DatatypeHandleIndex(3),
                field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                    name: IdentifierIndex(5),
                    signature: TypeSignature(SignatureToken::TypeParameter(0)),
                }]),
            },
        ],
        function_handles: vec![
            // fun fn()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(6),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
            // fun g_fn<T: key>()
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(7),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![AbilitySet::EMPTY | Ability::Key],
            },
            // fun test_fn(Sender)
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(8),
                parameters: SignatureIndex(1),
                return_: SignatureIndex(0),
                type_parameters: vec![],
            },
        ],
        function_defs: vec![
            // public fun fn() { return; }
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
            // fun g_fn<T>() { return; }
            FunctionDefinition {
                function: FunctionHandleIndex(1),
                visibility: Visibility::Private,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![Bytecode::Ret],
                    jump_tables: vec![],
                }),
            },
            // fun test_fn() { ... } - tests will fill up the code
            FunctionDefinition {
                function: FunctionHandleIndex(2),
                visibility: Visibility::Private,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(0),
                    code: vec![],
                    jump_tables: vec![],
                }),
            },
        ],
        signatures: vec![
            Signature(vec![]),                       // void
            Signature(vec![SignatureToken::Signer]), // Signer
        ],
        constant_pool: vec![
            // an address
            Constant {
                type_: SignatureToken::Address,
                data: AccountAddress::random().to_vec(),
            },
        ],
        metadata: vec![],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
        variant_handles: vec![],
        variant_instantiation_handles: vec![],
    }
}

#[test]
fn generic_call_to_non_generic_func() {
    let mut module = make_module();
    // bogus `CallGeneric fn()`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::CallGeneric(FunctionInstantiationIndex(0)),
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module.function_instantiations.push(FunctionInstantiation {
        handle: FunctionHandleIndex(0),
        type_parameters: SignatureIndex(2),
    });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("CallGeneric to non generic function must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn non_generic_call_to_generic_func() {
    let mut module = make_module();
    // bogus `Call g_fn<T>()`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![Bytecode::Call(FunctionHandleIndex(1)), Bytecode::Ret],
        jump_tables: vec![],
    });
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("Call to generic function must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn generic_pack_on_non_generic_struct() {
    let mut module = make_module();
    // bogus `PackGeneric S`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::PackGeneric(StructDefInstantiationIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module
        .struct_def_instantiations
        .push(StructDefInstantiation {
            def: StructDefinitionIndex(0),
            type_parameters: SignatureIndex(2),
        });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("PackGeneric to non generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn non_generic_pack_on_generic_struct() {
    let mut module = make_module();
    // bogus `Pack GS<T>`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::Pack(StructDefinitionIndex(1)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("Pack to generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn generic_unpack_on_non_generic_struct() {
    let mut module = make_module();
    // bogus `UnpackGeneric S`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::Pack(StructDefinitionIndex(0)),
            Bytecode::UnpackGeneric(StructDefInstantiationIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module
        .struct_def_instantiations
        .push(StructDefInstantiation {
            def: StructDefinitionIndex(0),
            type_parameters: SignatureIndex(2),
        });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("UnpackGeneric to non generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn non_generic_unpack_on_generic_struct() {
    let mut module = make_module();
    // bogus `Unpack GS<T>`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::PackGeneric(StructDefInstantiationIndex(0)),
            Bytecode::Unpack(StructDefinitionIndex(1)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module
        .struct_def_instantiations
        .push(StructDefInstantiation {
            def: StructDefinitionIndex(1),
            type_parameters: SignatureIndex(2),
        });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("Unpack to generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn generic_mut_borrow_field_on_non_generic_struct() {
    let mut module = make_module();
    // bogus `MutBorrowFieldGeneric S.t`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::Pack(StructDefinitionIndex(0)),
            Bytecode::MutBorrowFieldGeneric(FieldInstantiationIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module.field_instantiations.push(FieldInstantiation {
        handle: FieldHandleIndex(0),
        type_parameters: SignatureIndex(2),
    });
    module.field_handles.push(FieldHandle {
        owner: StructDefinitionIndex(0),
        field: 0,
    });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("MutBorrowFieldGeneric to non generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn non_generic_mut_borrow_field_on_generic_struct() {
    let mut module = make_module();
    // bogus `MutBorrowField GS<T>.f`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::PackGeneric(StructDefInstantiationIndex(0)),
            Bytecode::MutBorrowField(FieldHandleIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module
        .struct_def_instantiations
        .push(StructDefInstantiation {
            def: StructDefinitionIndex(1),
            type_parameters: SignatureIndex(2),
        });
    module.field_handles.push(FieldHandle {
        owner: StructDefinitionIndex(1),
        field: 0,
    });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("MutBorrowField to generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn generic_borrow_field_on_non_generic_struct() {
    let mut module = make_module();
    // bogus `ImmBorrowFieldGeneric S.f`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::Pack(StructDefinitionIndex(0)),
            Bytecode::ImmBorrowFieldGeneric(FieldInstantiationIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module.field_instantiations.push(FieldInstantiation {
        handle: FieldHandleIndex(0),
        type_parameters: SignatureIndex(2),
    });
    module.field_handles.push(FieldHandle {
        owner: StructDefinitionIndex(0),
        field: 0,
    });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("ImmBorrowFieldGeneric to non generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}

#[test]
fn non_generic_borrow_field_on_generic_struct() {
    let mut module = make_module();
    // bogus `ImmBorrowField GS<T>.f`
    module.function_defs[2].code = Some(CodeUnit {
        locals: SignatureIndex(0),
        code: vec![
            Bytecode::LdU64(10),
            Bytecode::PackGeneric(StructDefInstantiationIndex(0)),
            Bytecode::ImmBorrowField(FieldHandleIndex(0)),
            Bytecode::Pop,
            Bytecode::Ret,
        ],
        jump_tables: vec![],
    });
    module
        .struct_def_instantiations
        .push(StructDefInstantiation {
            def: StructDefinitionIndex(1),
            type_parameters: SignatureIndex(2),
        });
    module.field_handles.push(FieldHandle {
        owner: StructDefinitionIndex(1),
        field: 0,
    });
    module.signatures.push(Signature(vec![SignatureToken::U64]));
    let err = InstructionConsistency::verify_module(&module)
        .expect_err("ImmBorrowField to generic struct must fail");
    assert_eq!(
        err.major_status(),
        StatusCode::GENERIC_MEMBER_OPCODE_MISMATCH
    );
}
