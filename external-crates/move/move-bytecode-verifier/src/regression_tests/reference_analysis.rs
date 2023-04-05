// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    file_format::{
        empty_module, AbilitySet, AddressIdentifierIndex, Bytecode::*, CodeUnit, Constant,
        FieldDefinition, FunctionDefinition, FunctionHandle, FunctionHandleIndex, IdentifierIndex,
        ModuleHandle, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken::*,
        StructDefinition, StructDefinitionIndex, StructFieldInformation, StructHandle,
        StructHandleIndex, TypeSignature, Visibility, Visibility::*,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};
use std::str::FromStr;

#[test]
fn unbalanced_stack_crash() {
    let mut module = empty_module();
    module.version = 5;

    module.struct_handles.push(StructHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        abilities: AbilitySet::ALL,
        type_parameters: vec![],
    });

    let fun_handle = FunctionHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(2),
        parameters: SignatureIndex(0),
        return_: SignatureIndex(1),
        type_parameters: vec![],
    };

    module.function_handles.push(fun_handle);

    module.signatures.pop();
    module.signatures.push(Signature(vec![
        Address, U64, Address, Address, U128, Address, U64, U64, U64,
    ]));
    module.signatures.push(Signature(vec![]));
    module
        .signatures
        .push(Signature(vec![Address, Bool, Address]));

    module.identifiers.extend(
        vec![
            Identifier::from_str("zf_hello_world").unwrap(),
            Identifier::from_str("awldFnU18mlDKQfh6qNfBGx8X").unwrap(),
            Identifier::from_str("aQPwJNHyAHpvJ").unwrap(),
            Identifier::from_str("aT7ZphKTrKcYCwCebJySrmrKlckmnL5").unwrap(),
            Identifier::from_str("arYpsFa2fvrpPJ").unwrap(),
        ]
        .into_iter(),
    );
    module.address_identifiers.push(AccountAddress::random());

    module.constant_pool.push(Constant {
        type_: Address,
        data: AccountAddress::ZERO.into_bytes().to_vec(),
    });

    module.struct_defs.push(StructDefinition {
        struct_handle: StructHandleIndex(0),
        field_information: StructFieldInformation::Declared(vec![FieldDefinition {
            name: IdentifierIndex::new(3),
            signature: TypeSignature(Address),
        }]),
    });

    let code_unit = CodeUnit {
        code: vec![
            LdFalse,
            BrTrue(13),
            MoveLoc(3),
            MutBorrowGlobal(StructDefinitionIndex(0)),
            MoveLoc(6),
            Pop,
            MoveLoc(5),
            MutBorrowGlobal(StructDefinitionIndex(0)),
            MoveLoc(0),
            MutBorrowGlobal(StructDefinitionIndex(0)),
            Pop,
            Pop,
            Pop,
            Ret,
        ],
        locals: SignatureIndex::new(2),
    };
    let fun_def = FunctionDefinition {
        code: Some(code_unit),
        function: FunctionHandleIndex(0),
        visibility: Visibility::Public,
        is_entry: false,
        acquires_global_resources: vec![],
    };

    module.function_defs.push(fun_def);
    match crate::verify_module(&module) {
        Ok(_) => {}
        Err(e) => assert_eq!(e.major_status(), StatusCode::GLOBAL_REFERENCE_ERROR),
    }
}

#[test]
fn too_many_locals() {
    // Create a signature of 128 elements. This will be used both for locals and parameters,
    // thus the overall size will be 256. If this is not intercepted in bounds checks,
    // as a result the following iterator in abstract state
    // would be empty, breaking reference analysis: `0..self.num_locals as LocalIndex`
    // (since LocalIndex is u8).
    let sign_128 = (0..128)
        .map(|_| Reference(Box::new(U64)))
        .collect::<Vec<_>>();
    let module = CompiledModule {
        version: 5,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        struct_handles: vec![],
        function_handles: vec![FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![AbilitySet::ALL],
        }],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![Signature(sign_128)],
        identifiers: vec![Identifier::new("x").unwrap()],
        address_identifiers: vec![AccountAddress::ONE],
        constant_pool: vec![],
        metadata: vec![],
        struct_defs: vec![],
        function_defs: vec![FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility: Public,
            is_entry: true,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![CopyLoc(2), StLoc(33), Branch(0)],
            }),
        }],
    };

    let res = crate::verify_module(&module);

    match res {
        Ok(_) => {}
        Err(e) => assert_eq!(e.major_status(), StatusCode::TOO_MANY_LOCALS),
    }
}

#[test]
fn borrow_graph() {
    let module = CompiledModule {
        version: 5,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        struct_handles: vec![],
        function_handles: vec![FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(0),
            parameters: SignatureIndex(0),
            return_: SignatureIndex(0),
            type_parameters: vec![],
        }],
        field_handles: vec![],
        friend_decls: vec![],
        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        signatures: vec![Signature(vec![
            Reference(Box::new(U64)),
            Reference(Box::new(U64)),
        ])],
        identifiers: vec![Identifier::new("a").unwrap()],
        address_identifiers: vec![AccountAddress::ONE],
        constant_pool: vec![],
        metadata: vec![],
        struct_defs: vec![],
        function_defs: vec![FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility: Visibility::Public,
            is_entry: false,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![MoveLoc(0), MoveLoc(1), StLoc(0), StLoc(1), Branch(0)],
            }),
        }],
    };

    let res = crate::verify_module(&module);
    assert!(res.is_ok());
}
