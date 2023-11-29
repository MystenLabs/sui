// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use std::collections::HashMap;

use crate::move_vm::MoveVM;
use move_binary_format::{
    errors::{VMError, VMResult},
    file_format::{
        empty_module, AbilitySet, AddressIdentifierIndex, Bytecode, CodeUnit, CompiledModule,
        DatatypeHandle, DatatypeHandleIndex, FieldDefinition, FunctionDefinition, FunctionHandle,
        FunctionHandleIndex, IdentifierIndex, ModuleHandle, ModuleHandleIndex, Signature,
        SignatureIndex, SignatureToken, StructDefinition, StructFieldInformation, TableIndex,
        TypeSignature, Visibility,
    },
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::{IdentStr, Identifier},
    language_storage::{ModuleId, StructTag, TypeTag},
    resolver::{LinkageResolver, ModuleResolver, ResourceResolver},
    runtime_value::{serialize_values, MoveValue},
    u256::U256,
    vm_status::{StatusCode, StatusType},
};
use move_vm_types::gas::UnmeteredGasMeter;

fn make_module_with_function(
    visibility: Visibility,
    is_entry: bool,
    parameters: Signature,
    return_: Signature,
    type_parameters: Vec<AbilitySet>,
) -> (CompiledModule, Identifier) {
    let function_name = Identifier::new("foo").unwrap();
    let mut signatures = vec![Signature(vec![])];
    let parameters_idx = match signatures
        .iter()
        .enumerate()
        .find(|(_, s)| *s == &parameters)
    {
        Some((idx, _)) => SignatureIndex(idx as TableIndex),
        None => {
            signatures.push(parameters);
            SignatureIndex((signatures.len() - 1) as TableIndex)
        }
    };
    let return_idx = match signatures.iter().enumerate().find(|(_, s)| *s == &return_) {
        Some((idx, _)) => SignatureIndex(idx as TableIndex),
        None => {
            signatures.push(return_);
            SignatureIndex((signatures.len() - 1) as TableIndex)
        }
    };
    let module = CompiledModule {
        version: move_binary_format::file_format_common::VERSION_MAX,
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        datatype_handles: vec![DatatypeHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(1),
            abilities: AbilitySet::EMPTY,
            type_parameters: vec![],
        }],
        function_handles: vec![FunctionHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(2),
            parameters: parameters_idx,
            return_: return_idx,
            type_parameters,
        }],
        field_handles: vec![],
        friend_decls: vec![],

        struct_def_instantiations: vec![],
        function_instantiations: vec![],
        field_instantiations: vec![],
        enum_defs: vec![],
        enum_def_instantiations: vec![],

        signatures,

        identifiers: vec![
            Identifier::new("M").unwrap(),
            Identifier::new("X").unwrap(),
            function_name.clone(),
        ],
        address_identifiers: vec![AccountAddress::random()],
        constant_pool: vec![],
        metadata: vec![],

        struct_defs: vec![StructDefinition {
            struct_handle: DatatypeHandleIndex(0),
            field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                name: IdentifierIndex(1),
                signature: TypeSignature(SignatureToken::Bool),
            }]),
        }],
        function_defs: vec![FunctionDefinition {
            function: FunctionHandleIndex(0),
            visibility,
            is_entry,
            acquires_global_resources: vec![],
            code: Some(CodeUnit {
                locals: SignatureIndex(0),
                code: vec![Bytecode::LdU64(0), Bytecode::Abort],
                jump_tables: vec![],
            }),
        }],
    };
    (module, function_name)
}

// make a script function with a given signature for main.
fn make_script_function(signature: Signature) -> (CompiledModule, Identifier) {
    make_module_with_function(
        Visibility::Public,
        true,
        signature,
        Signature(vec![]),
        vec![],
    )
}

struct RemoteStore {
    modules: HashMap<ModuleId, Vec<u8>>,
}

impl RemoteStore {
    fn new() -> Self {
        Self {
            modules: HashMap::new(),
        }
    }

    fn add_module(&mut self, compiled_module: CompiledModule) {
        let id = compiled_module.self_id();
        let mut bytes = vec![];
        compiled_module.serialize(&mut bytes).unwrap();
        self.modules.insert(id, bytes);
    }
}

impl LinkageResolver for RemoteStore {
    type Error = VMError;
}

impl ModuleResolver for RemoteStore {
    type Error = VMError;
    fn get_module(&self, module_id: &ModuleId) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(self.modules.get(module_id).cloned())
    }
}

impl ResourceResolver for RemoteStore {
    type Error = VMError;

    fn get_resource(
        &self,
        _address: &AccountAddress,
        _tag: &StructTag,
    ) -> Result<Option<Vec<u8>>, Self::Error> {
        Ok(None)
    }
}

fn combine_signers_and_args(
    signers: Vec<AccountAddress>,
    non_signer_args: Vec<Vec<u8>>,
) -> Vec<Vec<u8>> {
    signers
        .into_iter()
        .map(|s| MoveValue::Signer(s).simple_serialize().unwrap())
        .chain(non_signer_args)
        .collect()
}

fn call_script_function_with_args_ty_args_signers(
    module: CompiledModule,
    function_name: Identifier,
    non_signer_args: Vec<Vec<u8>>,
    ty_arg_tags: Vec<TypeTag>,
    signers: Vec<AccountAddress>,
) -> VMResult<()> {
    let move_vm = MoveVM::new(vec![]).unwrap();
    let mut remote_view = RemoteStore::new();
    let id = module.self_id();
    remote_view.add_module(module);
    let mut session = move_vm.new_session(&remote_view);

    let ty_args = ty_arg_tags
        .into_iter()
        .map(|tag| session.load_type(&tag))
        .collect::<VMResult<_>>()?;

    session.execute_function_bypass_visibility(
        &id,
        function_name.as_ident_str(),
        ty_args,
        combine_signers_and_args(signers, non_signer_args),
        &mut UnmeteredGasMeter,
    )?;
    Ok(())
}

fn call_script_function(
    module: CompiledModule,
    function_name: Identifier,
    args: Vec<Vec<u8>>,
) -> VMResult<()> {
    call_script_function_with_args_ty_args_signers(module, function_name, args, vec![], vec![])
}

// these signatures used to be bad, but there are no bad signatures for scripts at the VM
fn deprecated_bad_signatures() -> Vec<Signature> {
    vec![
        // struct in signature
        Signature(vec![SignatureToken::Datatype(DatatypeHandleIndex(0))]),
        // struct in signature
        Signature(vec![
            SignatureToken::Bool,
            SignatureToken::Datatype(DatatypeHandleIndex(0)),
            SignatureToken::U64,
        ]),
        // reference to struct in signature
        Signature(vec![
            SignatureToken::Address,
            SignatureToken::MutableReference(Box::new(SignatureToken::Datatype(
                DatatypeHandleIndex(0),
            ))),
        ]),
        // vector of struct in signature
        Signature(vec![
            SignatureToken::Bool,
            SignatureToken::Vector(Box::new(SignatureToken::Datatype(DatatypeHandleIndex(0)))),
            SignatureToken::U64,
        ]),
        // vector of vector of struct in signature
        Signature(vec![
            SignatureToken::Bool,
            SignatureToken::Vector(Box::new(SignatureToken::Vector(Box::new(
                SignatureToken::Datatype(DatatypeHandleIndex(0)),
            )))),
            SignatureToken::U64,
        ]),
        // reference to vector in signature
        Signature(vec![SignatureToken::Reference(Box::new(
            SignatureToken::Vector(Box::new(SignatureToken::Datatype(DatatypeHandleIndex(0)))),
        ))]),
        // reference to vector in signature
        Signature(vec![SignatureToken::Reference(Box::new(
            SignatureToken::U64,
        ))]),
        // `&Signer` in signature (not `Signer`)
        Signature(vec![SignatureToken::Reference(Box::new(
            SignatureToken::Signer,
        ))]),
        // vector of `Signer` in signature
        Signature(vec![SignatureToken::Vector(Box::new(
            SignatureToken::Signer,
        ))]),
        // `Signer` ref not first arg
        Signature(vec![SignatureToken::Bool, SignatureToken::Signer]),
    ]
}

fn good_signatures_and_arguments() -> Vec<(Signature, Vec<MoveValue>)> {
    vec![
        // U128 arg
        (
            Signature(vec![SignatureToken::U128]),
            vec![MoveValue::U128(0)],
        ),
        // U8 arg
        (Signature(vec![SignatureToken::U8]), vec![MoveValue::U8(0)]),
        // U16 arg
        (
            Signature(vec![SignatureToken::U16]),
            vec![MoveValue::U16(0)],
        ),
        // U32 arg
        (
            Signature(vec![SignatureToken::U32]),
            vec![MoveValue::U32(0)],
        ),
        // U256 arg
        (
            Signature(vec![SignatureToken::U256]),
            vec![MoveValue::U256(U256::zero())],
        ),
        // All constants
        (
            Signature(vec![SignatureToken::Vector(Box::new(SignatureToken::Bool))]),
            vec![MoveValue::Vector(vec![
                MoveValue::Bool(false),
                MoveValue::Bool(true),
            ])],
        ),
        // All constants
        (
            Signature(vec![
                SignatureToken::Bool,
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
                SignatureToken::Address,
            ]),
            vec![
                MoveValue::Bool(true),
                MoveValue::vector_u8(vec![0, 1]),
                MoveValue::Address(AccountAddress::random()),
            ],
        ),
        // vector<vector<address>>
        (
            Signature(vec![
                SignatureToken::Bool,
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
                SignatureToken::Vector(Box::new(SignatureToken::Vector(Box::new(
                    SignatureToken::Address,
                )))),
            ]),
            vec![
                MoveValue::Bool(true),
                MoveValue::vector_u8(vec![0, 1]),
                MoveValue::Vector(vec![
                    MoveValue::Vector(vec![
                        MoveValue::Address(AccountAddress::random()),
                        MoveValue::Address(AccountAddress::random()),
                    ]),
                    MoveValue::Vector(vec![
                        MoveValue::Address(AccountAddress::random()),
                        MoveValue::Address(AccountAddress::random()),
                    ]),
                    MoveValue::Vector(vec![
                        MoveValue::Address(AccountAddress::random()),
                        MoveValue::Address(AccountAddress::random()),
                    ]),
                ]),
            ],
        ),
        //
        // Vector arguments
        //
        // empty vector
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Address,
            ))]),
            vec![MoveValue::Vector(vec![])],
        ),
        // one elem vector
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Address,
            ))]),
            vec![MoveValue::Vector(vec![MoveValue::Address(
                AccountAddress::random(),
            )])],
        ),
        // multiple elems vector
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Address,
            ))]),
            vec![MoveValue::Vector(vec![
                MoveValue::Address(AccountAddress::random()),
                MoveValue::Address(AccountAddress::random()),
                MoveValue::Address(AccountAddress::random()),
                MoveValue::Address(AccountAddress::random()),
                MoveValue::Address(AccountAddress::random()),
            ])],
        ),
        // empty vector of vector
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
            ))]),
            vec![MoveValue::Vector(vec![])],
        ),
        // multiple element vector of vector
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
            ))]),
            vec![MoveValue::Vector(vec![
                MoveValue::vector_u8(vec![0, 1]),
                MoveValue::vector_u8(vec![2, 3]),
                MoveValue::vector_u8(vec![4, 5]),
            ])],
        ),
    ]
}

fn mismatched_cases() -> Vec<(Signature, Vec<MoveValue>, StatusCode)> {
    vec![
        // Too few args
        (
            Signature(vec![SignatureToken::U64]),
            vec![],
            StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
        ),
        // Too many args
        (
            Signature(vec![SignatureToken::Bool]),
            vec![
                MoveValue::Bool(false),
                MoveValue::Bool(false),
                MoveValue::Bool(false),
            ],
            StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH,
        ),
        // Vec<bool> passed for vec<address>
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Address,
            ))]),
            vec![MoveValue::Vector(vec![MoveValue::Bool(true)])],
            StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
        ),
        // u128 passed for vec<address>
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Address,
            ))]),
            vec![MoveValue::U128(12)],
            StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
        ),
        // u8 passed for vector<vector<u8>>
        (
            Signature(vec![SignatureToken::Vector(Box::new(
                SignatureToken::Vector(Box::new(SignatureToken::U8)),
            ))]),
            vec![MoveValue::U8(12)],
            StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT,
        ),
    ]
}

fn general_cases() -> Vec<(
    Signature,
    Vec<MoveValue>,
    Vec<AccountAddress>,
    Option<StatusCode>,
)> {
    vec![
        // too few signers (0)
        (
            Signature(vec![SignatureToken::Signer, SignatureToken::Signer]),
            vec![],
            vec![],
            Some(StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH),
        ),
        // too few signers (1)
        (
            Signature(vec![SignatureToken::Signer, SignatureToken::Signer]),
            vec![],
            vec![AccountAddress::random()],
            Some(StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH),
        ),
        // too few signers (3)
        (
            Signature(vec![SignatureToken::Signer, SignatureToken::Signer]),
            vec![],
            vec![
                AccountAddress::random(),
                AccountAddress::random(),
                AccountAddress::random(),
            ],
            Some(StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH),
        ),
        // correct number of signers (2)
        (
            Signature(vec![SignatureToken::Signer, SignatureToken::Signer]),
            vec![],
            vec![AccountAddress::random(), AccountAddress::random()],
            None,
        ),
        // too many signers (1) in a script that expects 0 is no longer ok
        (
            Signature(vec![SignatureToken::U8]),
            vec![MoveValue::U8(0)],
            vec![AccountAddress::random()],
            Some(StatusCode::NUMBER_OF_ARGUMENTS_MISMATCH),
        ),
        // signer
        (
            Signature(vec![
                SignatureToken::Signer,
                SignatureToken::Bool,
                SignatureToken::Address,
            ]),
            vec![
                MoveValue::Bool(false),
                MoveValue::Address(AccountAddress::random()),
            ],
            vec![AccountAddress::random()],
            None,
        ),
    ]
}

#[test]
fn check_script_function() {
    //
    // Bad signatures
    //
    for signature in deprecated_bad_signatures() {
        let num_args = signature.0.len();
        let dummy_args = vec![MoveValue::Bool(false); num_args];
        let (module, function_name) = make_script_function(signature);
        let res = call_script_function(module, function_name, serialize_values(&dummy_args))
            .err()
            .unwrap();
        // either the dummy arg matches so abort, or it fails to match
        // but the important thing is that the signature was accepted
        assert!(
            res.major_status() == StatusCode::ABORTED
                || res.major_status() == StatusCode::FAILED_TO_DESERIALIZE_ARGUMENT
        )
    }

    //
    // Good signatures
    //
    for (signature, args) in good_signatures_and_arguments() {
        // Body of the script is just an abort, so `ABORTED` means the script was accepted and ran
        let expected_status = StatusCode::ABORTED;
        let (module, function_name) = make_script_function(signature);
        assert_eq!(
            call_script_function(module, function_name, serialize_values(&args))
                .err()
                .unwrap()
                .major_status(),
            expected_status
        )
    }

    //
    // Mismatched Cases
    //
    for (signature, args, error) in mismatched_cases() {
        let (module, function_name) = make_script_function(signature);
        assert_eq!(
            call_script_function(module, function_name, serialize_values(&args))
                .err()
                .unwrap()
                .major_status(),
            error
        );
    }

    for (signature, args, signers, expected_status_opt) in general_cases() {
        // Body of the script is just an abort, so `ABORTED` means the script was accepted and ran
        let expected_status = expected_status_opt.unwrap_or(StatusCode::ABORTED);
        let (module, function_name) = make_script_function(signature);
        assert_eq!(
            call_script_function_with_args_ty_args_signers(
                module,
                function_name,
                serialize_values(&args),
                vec![],
                signers
            )
            .err()
            .unwrap()
            .major_status(),
            expected_status
        );
    }

    //
    // Non script visible
    // DEPRECATED this check must now be done by the adapter
    //
    // public
    let (module, function_name) = make_module_with_function(
        Visibility::Public,
        false,
        Signature(vec![]),
        Signature(vec![]),
        vec![],
    );
    assert_eq!(
        call_script_function_with_args_ty_args_signers(
            module,
            function_name,
            vec![],
            vec![],
            vec![],
        )
        .err()
        .unwrap()
        .major_status(),
        StatusCode::ABORTED,
    );
    // private
    let (module, function_name) = make_module_with_function(
        Visibility::Private,
        false,
        Signature(vec![]),
        Signature(vec![]),
        vec![],
    );
    assert_eq!(
        call_script_function_with_args_ty_args_signers(
            module,
            function_name,
            vec![],
            vec![],
            vec![],
        )
        .err()
        .unwrap()
        .major_status(),
        StatusCode::ABORTED,
    );
}

#[test]
fn call_missing_item() {
    let module = empty_module();
    let id = &module.self_id();
    let function_name = IdentStr::new("foo").unwrap();
    // mising module
    let move_vm = MoveVM::new(vec![]).unwrap();
    let mut remote_view = RemoteStore::new();
    let mut session = move_vm.new_session(&remote_view);
    let error = session
        .execute_function_bypass_visibility(
            id,
            function_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
        )
        .err()
        .unwrap();
    assert_eq!(error.major_status(), StatusCode::LINKER_ERROR);
    assert_eq!(error.status_type(), StatusType::Verification);
    drop(session);

    // missing function
    remote_view.add_module(module);
    let mut session = move_vm.new_session(&remote_view);
    let error = session
        .execute_function_bypass_visibility(
            id,
            function_name,
            vec![],
            Vec::<Vec<u8>>::new(),
            &mut UnmeteredGasMeter,
        )
        .err()
        .unwrap();
    assert_eq!(
        error.major_status(),
        StatusCode::FUNCTION_RESOLUTION_FAILURE
    );
    assert_eq!(error.status_type(), StatusType::Verification);
}
