// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![no_main]
use move_binary_format::file_format::{
    empty_module, AbilitySet, Bytecode, CodeUnit, Constant, DatatypeHandle, DatatypeHandleIndex,
    FieldDefinition, FunctionDefinition, FunctionHandle, FunctionHandleIndex, IdentifierIndex,
    ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
    SignatureToken::{Address, Bool},
    StructDefinition, StructFieldInformation, TypeSignature, Visibility,
};
use move_core_types::{account_address::AccountAddress, identifier::Identifier};
use std::str::FromStr;

use arbitrary::Arbitrary;
use libfuzzer_sys::fuzz_target;

#[derive(Arbitrary, Debug)]
struct Mixed {
    code: Vec<Bytecode>,
    abilities: AbilitySet,
    param_types: Vec<SignatureToken>,
    return_type: Option<SignatureToken>,
}

fuzz_target!(|mix: Mixed| {
    let mut module = empty_module();
    module.version = 5;

    module.datatype_handles.push(DatatypeHandle {
        module: ModuleHandleIndex(0),
        name: IdentifierIndex(1),
        abilities: mix.abilities,
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
    module.signatures.push(Signature(mix.param_types));
    module.signatures.push(Signature(
        mix.return_type.map(|s| vec![s]).unwrap_or_default(),
    ));
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
        code: mix.code,
        locals: SignatureIndex(0),
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
    let _ = move_bytecode_verifier::verify_module_unmetered(&module);
});
