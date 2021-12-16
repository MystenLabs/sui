use fastx_types::FASTX_FRAMEWORK_ADDRESS;
use fastx_verifier::id_leak_verifier::verify_module;
use move_binary_format::file_format::*;
use move_core_types::identifier::Identifier;

fn make_module() -> CompiledModule {
    /*
    We are setting up a module that looks like this:
    struct Foo has key {
        id: 0x1::ID::ID
    }
    */
    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![
            Identifier::new("ID").unwrap(), // ID Module name as well as struct name
            Identifier::new("S").unwrap(),  // Test struct name
            Identifier::new("id").unwrap(), // id field
            Identifier::new("foo").unwrap(), // Test function name
            Identifier::new("transfer").unwrap(),
        ],
        address_identifiers: vec![FASTX_FRAMEWORK_ADDRESS],
        struct_handles: vec![
            // The FASTX_FRAMEWORK_ADDRESS::ID::ID struct
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(0),
                abilities: AbilitySet::EMPTY | Ability::Store | Ability::Drop,
                type_parameters: vec![],
            },
            // A struct with key ability
            StructHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(1),
                abilities: AbilitySet::EMPTY | Ability::Key,
                type_parameters: vec![],
            },
        ],
        struct_defs: vec![StructDefinition {
            struct_handle: StructHandleIndex(1),
            field_information: StructFieldInformation::Declared(vec![
                // id field.
                FieldDefinition {
                    name: IdentifierIndex(2),
                    signature: TypeSignature(SignatureToken::Struct(StructHandleIndex(0))),
                },
            ]),
        }],
        function_handles: vec![],
        function_defs: vec![],
        signatures: vec![
            Signature(vec![]),                                             // void
            Signature(vec![SignatureToken::Struct(StructHandleIndex(0))]), // (ID)
            Signature(vec![SignatureToken::Struct(StructHandleIndex(1))]), // (S)
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
fn id_leak_through_direct_return() {
    /*
    fun foo(f: Foo): 0x1::ID::ID {
        let Foo { id: id } = f;
        return id;
    }
    */
    let mut module = make_module();
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(2),
        return_: SignatureIndex(1),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![
                Bytecode::MoveLoc(0),
                Bytecode::Unpack(StructDefinitionIndex(0)),
                Bytecode::Ret,
            ],
        }),
    });
    let result = verify_module(&module);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function return."));
}

#[test]
fn id_leak_through_indirect_return() {
    /*
    fun foo(f: Foo): Foo {
        let Foo { id: id } = f;
        let r = Foo { id: id };
        return r;
    }
    */
    let mut module = make_module();
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(2),
        return_: SignatureIndex(1),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![
                Bytecode::MoveLoc(0),
                Bytecode::Unpack(StructDefinitionIndex(0)),
                Bytecode::Pack(StructDefinitionIndex(0)),
                Bytecode::Ret,
            ],
        }),
    });
    let result = verify_module(&module);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function return."));
}

#[test]
fn id_leak_through_reference() {
    /*
    fun foo(f: Foo, ref: &mut 0x1::ID::ID) {
        let Foo { id: id } = f;
        *ref = id;
    }
    */
    let mut module = make_module();
    module.signatures.push(Signature(vec![
        SignatureToken::Struct(StructHandleIndex(1)),
        SignatureToken::MutableReference(Box::new(SignatureToken::Struct(StructHandleIndex(0)))),
    ]));
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(3),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![
                Bytecode::MoveLoc(0),
                Bytecode::Unpack(StructDefinitionIndex(0)),
                Bytecode::MoveLoc(1),
                Bytecode::WriteRef,
                Bytecode::Ret,
            ],
        }),
    });
    let result = verify_module(&module);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID is leaked to a reference."));
}

#[test]
fn id_direct_leak_through_call() {
    /*
    fun transfer(id: 0x1::ID::ID);

    fun foo(f: Foo) {
        let Foo { id: id } = f;
        transfer(id);
    }
    */
    let mut module = make_module();
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(2),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    // A dummy transfer function.
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(4),
        parameters: SignatureIndex(1),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![
                Bytecode::MoveLoc(0),
                Bytecode::Unpack(StructDefinitionIndex(0)),
                Bytecode::Call(FunctionHandleIndex(1)),
            ],
        }),
    });
    let result = verify_module(&module);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function call."));
}

#[test]
fn id_indirect_leak_through_call() {
    /*
    fun transfer(f: Foo);

    fun foo(f: Foo) {
        let Foo { id: id } = f;
        let newf = Foo { id: id };
        transfer(newf);
    }
    */
    let mut module = make_module();
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(3),
        parameters: SignatureIndex(2),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    // A dummy transfer function.
    module.function_handles.push(FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(4),
        parameters: SignatureIndex(2),
        return_: SignatureIndex(0),
        type_parameters: vec![],
    });
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![
                Bytecode::MoveLoc(0),
                Bytecode::Unpack(StructDefinitionIndex(0)),
                Bytecode::Pack(StructDefinitionIndex(0)),
                Bytecode::Call(FunctionHandleIndex(1)),
            ],
        }),
    });
    let result = verify_module(&module);
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("ID leak detected in function foo: ID leaked through function call."));
}
