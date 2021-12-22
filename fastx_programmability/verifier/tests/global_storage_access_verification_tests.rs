use fastx_verifier::global_storage_access_verifier::verify_module;
use move_binary_format::file_format::*;
use move_core_types::{account_address::AccountAddress, identifier::Identifier};

fn make_module() -> CompiledModule {
    CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        self_module_handle_idx: ModuleHandleIndex(0),
        identifiers: vec![Identifier::new("foo").unwrap()],
        address_identifiers: vec![AccountAddress::new([0u8; AccountAddress::LENGTH])],
        struct_handles: vec![],
        struct_defs: vec![],
        function_handles: vec![FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        }],
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
fn function_with_global_access_bytecode() {
    let mut module = make_module();
    module.function_defs.push(FunctionDefinition {
        function: FunctionHandleIndex(0),
        visibility: Visibility::Private,
        acquires_global_resources: vec![],
        code: Some(CodeUnit {
            locals: SignatureIndex(0),
            code: vec![],
        }),
    });
    assert!(verify_module(&module).is_ok());
    let code = &mut module.function_defs[0].code.as_mut().unwrap().code;
    // All the bytecode that could access global storage.
    code.extend(vec![
        Bytecode::Exists(StructDefinitionIndex(0)),
        Bytecode::ImmBorrowGlobal(StructDefinitionIndex(0)),
        Bytecode::ImmBorrowGlobalGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MoveFrom(StructDefinitionIndex(0)),
        Bytecode::MoveFromGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MoveTo(StructDefinitionIndex(0)),
        Bytecode::MoveToGeneric(StructDefInstantiationIndex(0)),
        Bytecode::MutBorrowGlobal(StructDefinitionIndex(0)),
        Bytecode::MutBorrowGlobalGeneric(StructDefInstantiationIndex(0)),
    ]);
    let invalid_bytecode_str = format!("{:?}", code);
    // Add a few valid bytecode that doesn't access global storage.
    code.extend(vec![
        Bytecode::Add,
        Bytecode::ImmBorrowField(FieldHandleIndex(0)),
        Bytecode::Call(FunctionHandleIndex(0)),
    ]);
    assert!(verify_module(&module)
        .unwrap_err()
        .to_string()
        .contains(&format!(
            "Access to Move global storage is not allowed. Found in function foo: {}",
            invalid_bytecode_str
        )));
}
