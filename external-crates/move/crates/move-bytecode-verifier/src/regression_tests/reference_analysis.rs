// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::{
    file_format::{
        empty_module, AbilitySet, AddressIdentifierIndex,
        Bytecode::{self, *},
        CodeUnit, Constant, DatatypeHandle, DatatypeHandleIndex, FieldDefinition,
        FunctionDefinition, FunctionHandle, FunctionHandleIndex, IdentifierIndex, ModuleHandle,
        ModuleHandleIndex, Signature, SignatureIndex,
        SignatureToken::{self, *},
        StructDefinition, StructDefinitionIndex, StructFieldInformation, TypeSignature, Visibility,
        Visibility::*,
    },
    CompiledModule,
};
use move_core_types::{
    account_address::AccountAddress, identifier::Identifier, vm_status::StatusCode,
};
use move_vm_config::verifier::VerifierConfig;
use std::str::FromStr;

#[test]
fn unbalanced_stack_crash() {
    let mut module = empty_module();
    module.version = 5;

    module.datatype_handles.push(DatatypeHandle {
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
        struct_handle: DatatypeHandleIndex(0),
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
        jump_tables: vec![],
    };
    let fun_def = FunctionDefinition {
        code: Some(code_unit),
        function: FunctionHandleIndex(0),
        visibility: Visibility::Public,
        is_entry: false,
        acquires_global_resources: vec![],
    };

    module.function_defs.push(fun_def);
    match crate::verify_module_unmetered(&module) {
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
        datatype_handles: vec![],
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
                jump_tables: vec![],
            }),
        }],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
    };

    let res = crate::verify_module_unmetered(&module);

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
        datatype_handles: vec![],
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
                jump_tables: vec![],
            }),
        }],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
    };

    let res = crate::verify_module_unmetered(&module);
    assert!(res.is_ok());
}

#[test]
fn indirect_code() {
    use Bytecode::*;
    let v = 0;
    let v_ref = 1;
    let x = 2;
    let x_ref = 3;
    let vsig = SignatureIndex(2);
    let next = 16;
    let mut code = vec![
        // x = 10; x_ref = &mut x;
        LdU64(10),
        StLoc(x),
        MutBorrowLoc(x),
        StLoc(x_ref),
        // v = vector[100, 1000]; v_ref = &mut v
        LdU64(100),
        LdU64(1000),
        VecPack(vsig, 2),
        StLoc(v),
        MutBorrowLoc(v),
        StLoc(v_ref),
        // if (*x_ref == 0) return;
        CopyLoc(x_ref),
        ReadRef,
        LdU64(0),
        Eq,
        BrFalse(next),
        Ret,
    ];
    assert_eq!(code.len(), next as usize);
    code.extend(vec![
        // creates dangling reference on second iteration
        // _ = vec_pop_back(x_ref)
        CopyLoc(v_ref),
        VecPopBack(vsig),
        Pop,
        // *x_ref = 0
        LdU64(0),
        CopyLoc(x_ref),
        WriteRef,
        // x_ref = vec_mut_borrow(v_ref, 0);
        CopyLoc(v_ref),
        LdU64(0),
        VecMutBorrow(vsig),
        StLoc(x_ref),
    ]);
    let nops = vec![Nop; (u16::MAX as usize) - code.len() - 1];
    code.extend(nops);
    code.push(Branch(10));
    assert_eq!(code.len(), (u16::MAX as usize));
    let module = CompiledModule {
        version: 5,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        datatype_handles: vec![],
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
        signatures: vec![
            Signature(vec![]),
            Signature(vec![
                SignatureToken::Vector(Box::new(SignatureToken::U64)),
                SignatureToken::MutableReference(Box::new(SignatureToken::Vector(Box::new(
                    SignatureToken::U64,
                )))),
                SignatureToken::U64,
                SignatureToken::MutableReference(Box::new(SignatureToken::U64)),
            ]),
            Signature(vec![SignatureToken::U64]),
        ],
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
                locals: SignatureIndex(1),
                code,
                jump_tables: vec![],
            }),
        }],
        enum_defs: vec![],
        enum_def_instantiations: vec![],
    };

    let res = crate::verify_module_with_config_unmetered(&VerifierConfig::unbounded(), &module)
        .unwrap_err();
    assert_eq!(
        res.major_status(),
        StatusCode::VEC_UPDATE_EXISTS_MUTABLE_BORROW_ERROR
    );
}
