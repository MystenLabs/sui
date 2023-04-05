// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

#![allow(dead_code)]
#![allow(unused_must_use)]
#![allow(unused_imports)]

use move_binary_format::{
    errors::VMResult,
    file_format::{
        AbilitySet, AddressIdentifierIndex, Bytecode, Bytecode::*, CodeUnit, CompiledModule,
        Constant, ConstantPoolIndex, FieldDefinition, FunctionDefinition, FunctionHandle,
        FunctionHandleIndex, FunctionInstantiation, FunctionInstantiationIndex, IdentifierIndex,
        ModuleHandle, ModuleHandleIndex, Signature, SignatureIndex, SignatureToken,
        SignatureToken::*, StructDefInstantiation, StructDefInstantiationIndex, StructDefinition,
        StructDefinitionIndex, StructFieldInformation, StructHandle, StructHandleIndex,
        StructTypeParameter, TypeSignature,
    },
};
use move_core_types::{
    account_address::AccountAddress,
    identifier::Identifier,
    language_storage::{ModuleId, StructTag, TypeTag},
    vm_status::StatusCode,
};
use move_vm_runtime::{
    move_vm::MoveVM,
    session::{SerializedReturnValues, Session},
};
use move_vm_test_utils::{
    gas_schedule::{Gas, GasStatus, INITIAL_COST_SCHEDULE},
    InMemoryStorage,
};
use std::time::Instant;

const MODULE_NAME: &str = "Mod";
const STRUCT_NAME: &str = "S";
const FIELD_NAME: &str = "field";
const ENTRY_POINT_NAME: &str = "entry_point";
const RECURSIVE_NAME: &str = "recursive";
const EMPTY_NAME: &str = "empty";

fn main() {
    main_run(10000);
}

// Get a `GasStatus` to be used when running code.
// A `gas_val` of 0 returns an unmetered `GasStatus` which means code in this test will
// run forever (we always and only generate infinite loops)
fn get_gas_meter<'a>(gas_val: u64) -> GasStatus<'a> {
    if gas_val == 0 {
        GasStatus::new_unmetered()
    } else {
        GasStatus::new(&INITIAL_COST_SCHEDULE, Gas::new(gas_val))
    }
}

// With proper gas_val units this function can be used to profile the VM.
// All code generated is an infinite loop and with 0 unit we run forever which
// can be used for profiling
fn main_run(gas_val: u64) {
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), load_pop);
    println!(
        "* load_pop: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), vec_pack_instantiated);
    println!(
        "* vec_pack_instantiated: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), vec_pack_gen_simple);
    println!(
        "* vec_pack_gen_simple: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), vec_pack_gen_deep);
    println!(
        "* vec_pack_gen_deep: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), vec_pack_gen_deep_50);
    println!(
        "* vec_pack_gen_deep_50: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), vec_pack_gen_deep_500);
    println!(
        "* vec_pack_gen_deep_500: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), instantiated_gen_exists);
    println!(
        "* instantiated_gen_exists: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), simple_gen_exists);
    println!(
        "* simple_gen_exists: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_exists);
    println!(
        "* deep_gen_exists: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_exists_50);
    println!(
        "* deep_gen_exists_50: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_exists_500);
    println!(
        "* deep_gen_exists_500: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), instantiated_gen_call);
    println!(
        "* instantiated_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), simple_gen_call);
    println!(
        "* simple_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_call);
    println!(
        "* deep_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_call_50);
    println!(
        "* deep_gen_call_50: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_gen_call_500);
    println!(
        "* deep_gen_call_500: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), instantiated_rec_gen_call);
    println!(
        "* instantiated_rec_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), simple_rec_gen_call);
    println!(
        "* simple_rec_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
    let (res, time) = run_with_module(&mut get_gas_meter(gas_val), deep_rec_gen_call);
    println!(
        "* deep_rec_gen_call: {} - Status: {:?}",
        time,
        res.err().unwrap().major_status()
    );
}

#[test]
fn test_instantiation_no_instantiation() {
    let (res, ref_time) = run_with_module(&mut get_gas_meter(1000), load_pop);
    let err = res.err().unwrap().major_status();
    println!("* load_pop: {} - Status: {:?}", ref_time, err);
    assert_eq!(err, StatusCode::OUT_OF_GAS, "Must finish OutOfGas");
}

// Common runner for all tests.
// Run a control test (load_pop) and an instantiation test which is then
// compared against the control.
// Ensure that tests complete with "out of gas" and withing a given time range.
fn test_runner(
    gas_val: u64,
    test_name: &str,
    entry_spec: fn(
        AccountAddress,
        &mut Session<&'_ InMemoryStorage>,
    ) -> (ModuleId, Identifier, Vec<TypeTag>),
    check_result: fn(u128, u128) -> bool,
) {
    assert!(gas_val > 0, "Must provide a positive gas budget");
    let mut gas: GasStatus = get_gas_meter(gas_val);
    let (res, ref_time) = run_with_module(&mut gas, load_pop);
    assert_eq!(
        res.err().unwrap().major_status(),
        StatusCode::OUT_OF_GAS,
        "Must finish OutOfGas"
    );
    let mut gas: GasStatus = get_gas_meter(gas_val);
    let (res, time) = run_with_module(&mut gas, entry_spec);
    let err = res.err().unwrap().major_status();
    println!("* {}: {}ms - Status: {:?}", test_name, time, err);
    // assert_eq!(err, StatusCode::OUT_OF_GAS, "Must finish OutOfGas");
    assert!(
        check_result(time, ref_time),
        "Instantion test taking too long {}",
        time
    );
}

#[test]
fn test_instantiation_vec_pack_instantiated() {
    test_runner(
        1000,
        "vec_pack_instantiated",
        vec_pack_instantiated,
        |time, ref_time| time < ref_time * 10,
    );
}

#[test]
fn test_instantiation_vec_pack_gen_simple() {
    test_runner(
        1000,
        "vec_pack_gen_simple",
        vec_pack_gen_simple,
        |time, ref_time| time < ref_time * 10,
    );
}

#[test]
fn test_instantiation_vec_pack_gen_deep() {
    test_runner(
        1000,
        "vec_pack_gen_deep",
        vec_pack_gen_deep,
        |time, ref_time| time < ref_time * 1000,
    );
    test_runner(
        1000,
        "vec_pack_gen_deep_50",
        vec_pack_gen_deep_50,
        |time, ref_time| time < ref_time * 1000,
    );
    test_runner(
        1000,
        "vec_pack_gen_deep_500",
        vec_pack_gen_deep_500,
        |time, ref_time| time < ref_time * 1000,
    );
}

#[test]
fn test_instantiation_instantiated_gen_exists() {
    test_runner(
        1000,
        "instantiated_gen_exists",
        instantiated_gen_exists,
        |time, ref_time| time < ref_time * 10,
    );
}

#[test]
fn test_instantiation_simple_gen_exists() {
    test_runner(
        1000,
        "simple_gen_exists",
        simple_gen_exists,
        |time, ref_time| time < ref_time * 10,
    );
}

#[test]
fn test_instantiation_deep_gen_exists() {
    test_runner(
        1000,
        "deep_gen_exists",
        deep_gen_exists,
        |time, ref_time| time < ref_time * 1000,
    );
    test_runner(
        1000,
        "deep_gen_exists_50",
        deep_gen_exists_50,
        |time, ref_time| time < ref_time * 1000,
    );
    test_runner(
        1000,
        "deep_gen_exists_500",
        deep_gen_exists_500,
        |time, ref_time| time < ref_time * 1000,
    );
}

#[test]
fn test_instantiation_instantiated_gen_call() {
    test_runner(
        1000,
        "instantiated_gen_call",
        instantiated_gen_call,
        |time, ref_time| time < ref_time * 1000,
    );
}

#[test]
fn test_instantiation_simple_gen_call() {
    test_runner(
        1000,
        "simple_gen_call",
        simple_gen_call,
        |time, ref_time| time < ref_time * 1000,
    );
}

#[test]
fn test_instantiation_deep_gen_call() {
    test_runner(1000, "deep_gen_call", deep_gen_call, |time, ref_time| {
        time < ref_time * 100
    });
    test_runner(
        1000,
        "deep_gen_call_50",
        deep_gen_call_50,
        |time, ref_time| time < ref_time * 100,
    );
    test_runner(
        1000,
        "deep_gen_call_500",
        deep_gen_call_500,
        |time, ref_time| time < ref_time * 100,
    );
}

#[test]
fn test_instantiation_simple_rec_gen_call() {
    test_runner(
        1000,
        "simple_rec_gen_call",
        simple_rec_gen_call,
        |time, ref_time| time < ref_time * 10,
    );
}

#[test]
fn test_instantiation_deep_rec_gen_call() {
    test_runner(
        1000,
        "deep_rec_gen_call",
        deep_rec_gen_call,
        |time, ref_time| time < ref_time * 1000,
    );
}

// Generate a verifiable module with a snippet of code that can be used to test instantiations.
// The code is a block (so balanced stack) passed via `snippet` and it's repeated
// `snippet_rep` times. It is then completed with a `Branch(0)` to form an infinite
// loop. That can be used with no gas charge to profile the code executed (an executable),
// or with some gas to determine how long it takes to go OutOfGas.
// The code has to work on a `void(void)` function (so to speak) that has
// a number of type parameters defined via `func_type_params_count`.
// It can define a number of locals through `locals_sig`. The locals are added
// after the "default" `Signature` provided (look at that for the indexes that can be used).
// A struct is defined that uses `struct_type_params_count` to define the number of
// type parameters.
// A set of `Signature` can be provided for the code snippet to use besides the default
// and the locals.
// `struct_inst_signatures` and `func_inst_signatures` can be used to generate
// `StructDefInstantiation` and `FunctionInstantiation` that can be used by related bytecodes.
//
// Notice: this is not a particularly easy function to use. See example below on how to use it.
fn make_module(
    session: &mut Session<&'_ InMemoryStorage>,
    addr: AccountAddress,
    func_type_params_count: usize,
    locals_sig: Option<Signature>,
    snippet: Vec<Bytecode>,
    snippet_rep: usize,
    mut code_inst_signatures: Vec<Signature>,
    struct_type_params_count: usize,
    mut struct_inst_signatures: Vec<Signature>,
    func_handle_idxs: Vec<u16>,
    mut func_inst_signatures: Vec<Signature>,
) -> (ModuleId, Identifier) {
    // default signatures
    let mut signatures = vec![
        Signature(vec![]),
        Signature(vec![U64]),
        Signature(vec![TypeParameter(0)]),
    ];
    let locals_idx = if let Some(sig) = locals_sig {
        signatures.push(sig);
        signatures.len() - 1
    } else {
        0
    };
    signatures.append(&mut code_inst_signatures);
    let struct_inst_start = signatures.len();
    signatures.append(&mut struct_inst_signatures);
    let func_inst_start = signatures.len();
    signatures.append(&mut func_inst_signatures);

    let func_type_params = vec![AbilitySet::VECTOR; func_type_params_count];
    let rec_func_type_params = if func_type_params_count == 0 {
        vec![AbilitySet::VECTOR]
    } else {
        func_type_params.clone()
    };
    let struct_type_parameters = vec![
        StructTypeParameter {
            constraints: AbilitySet::EMPTY,
            is_phantom: false,
        };
        struct_type_params_count
    ];

    // create the code for the single entry point
    let mut code = vec![];
    for _ in 0..snippet_rep {
        code.append(&mut snippet.clone())
    }
    code.push(Branch(0));

    // struct definition instantiations
    let mut struct_def_instantiations = vec![];
    for idx in struct_inst_start..func_inst_start {
        struct_def_instantiations.push(StructDefInstantiation {
            def: StructDefinitionIndex(0),
            type_parameters: SignatureIndex(idx as u16),
        });
    }

    // function instantiations
    let entry_point = Identifier::new(ENTRY_POINT_NAME).unwrap();
    let mut function_instantiations = vec![FunctionInstantiation {
        handle: FunctionHandleIndex(1),
        type_parameters: SignatureIndex(2),
    }];
    for idx in func_inst_start..signatures.len() {
        function_instantiations.push(FunctionInstantiation {
            handle: FunctionHandleIndex(func_handle_idxs[idx - func_inst_start]),
            type_parameters: SignatureIndex(idx as u16),
        });
    }

    let module = CompiledModule {
        version: 6,
        // Module definition
        self_module_handle_idx: ModuleHandleIndex(0),
        module_handles: vec![ModuleHandle {
            address: AddressIdentifierIndex(0),
            name: IdentifierIndex(0),
        }],
        // struct definition
        struct_handles: vec![StructHandle {
            module: ModuleHandleIndex(0),
            name: IdentifierIndex(1),
            abilities: AbilitySet::ALL,
            type_parameters: struct_type_parameters,
        }],
        struct_defs: vec![StructDefinition {
            struct_handle: StructHandleIndex(0),
            field_information: StructFieldInformation::Declared(vec![FieldDefinition {
                name: IdentifierIndex(2),
                signature: TypeSignature(U8),
            }]),
        }],
        // function definition
        function_handles: vec![
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(3),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: func_type_params,
            },
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(4),
                parameters: SignatureIndex(1),
                return_: SignatureIndex(0),
                type_parameters: rec_func_type_params,
            },
            FunctionHandle {
                module: ModuleHandleIndex(0),
                name: IdentifierIndex(5),
                parameters: SignatureIndex(0),
                return_: SignatureIndex(0),
                type_parameters: vec![AbilitySet::VECTOR],
            },
        ],
        function_defs: vec![
            FunctionDefinition {
                function: FunctionHandleIndex(0),
                visibility: move_binary_format::file_format::Visibility::Public,
                is_entry: true,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(locals_idx as u16),
                    code,
                }),
            },
            FunctionDefinition {
                function: FunctionHandleIndex(1),
                visibility: move_binary_format::file_format::Visibility::Public,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(locals_idx as u16),
                    code: vec![
                        CopyLoc(0),
                        LdU64(1),
                        Add,
                        StLoc(0),
                        CopyLoc(0),
                        LdU64(10),
                        Ge,
                        BrTrue(10),
                        CopyLoc(0),
                        CallGeneric(FunctionInstantiationIndex(0)),
                        Ret,
                    ],
                }),
            },
            FunctionDefinition {
                function: FunctionHandleIndex(2),
                visibility: move_binary_format::file_format::Visibility::Public,
                is_entry: false,
                acquires_global_resources: vec![],
                code: Some(CodeUnit {
                    locals: SignatureIndex(locals_idx as u16),
                    code: vec![Ret],
                }),
            },
        ],
        // addresses
        address_identifiers: vec![addr],
        // identifiers
        identifiers: vec![
            // Module name
            Identifier::new(MODULE_NAME).unwrap(),
            // Struct name
            Identifier::new(STRUCT_NAME).unwrap(),
            // Field name
            Identifier::new(FIELD_NAME).unwrap(),
            // Entry point
            entry_point.clone(),
            // recursive fun name
            Identifier::new(RECURSIVE_NAME).unwrap(),
            // empty fun name
            Identifier::new(EMPTY_NAME).unwrap(),
        ],
        // constants
        constant_pool: vec![Constant {
            type_: Address,
            data: addr.to_vec(),
        }],
        // signatures
        signatures,
        // struct instantiations
        struct_def_instantiations,
        // function instantiations
        function_instantiations,
        // unused...
        field_handles: vec![],
        friend_decls: vec![],
        field_instantiations: vec![],
        metadata: vec![],
    };
    // uncomment to see the module generated
    // println!("Module: {:#?}", module);
    move_bytecode_verifier::verify_module(&module).expect("verification failed");

    let mut mod_bytes = vec![];
    module
        .serialize(&mut mod_bytes)
        .expect("Module must serialize");
    session
        .publish_module(mod_bytes, addr, &mut GasStatus::new_unmetered())
        .expect("Module must publish");
    (module.self_id(), entry_point)
}

// Generic function to run some code. Take the gas to use and a closure
// that can return an entry point to call.
// This function creates a VM, invokes the closure, and on return it builds the call
// for the entry point.
// Report time spent, if it terminates (no gas it will never end; use for profiling).
fn run_with_module(
    gas: &mut GasStatus,
    entry_spec: fn(
        AccountAddress,
        &mut Session<&'_ InMemoryStorage>,
    ) -> (ModuleId, Identifier, Vec<TypeTag>),
) -> (VMResult<SerializedReturnValues>, u128) {
    let addr = AccountAddress::from_hex_literal("0xcafe").unwrap();

    //
    // Start VM
    let vm = MoveVM::new(vec![]).unwrap();
    let storage: InMemoryStorage = InMemoryStorage::new();
    let mut session = vm.new_session(&storage);

    let (module_id, entry_name, type_arg_tags) = entry_spec(addr, &mut session);

    let now = Instant::now();

    let type_args = type_arg_tags
        .into_iter()
        .map(|tag| session.load_type(&tag))
        .collect::<VMResult<Vec<_>>>();

    let res = type_args.and_then(|type_args| {
        session.execute_entry_function(
            &module_id,
            entry_name.as_ref(),
            type_args,
            Vec::<Vec<u8>>::new(),
            gas,
        )
    });

    let time = now.elapsed().as_millis();
    (res, time)
}

// Call a simple load u8 and pop loop
fn load_pop(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 0;
    let locals_sig = None;
    let snippet = get_load_pop();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![0];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![])
}

// Code for LdU8 and Pop
fn get_load_pop() -> Vec<Bytecode> {
    vec![LdU8(0), Pop]
}

// Call a vector<T> pack empty and pop with full instantiation
fn vec_pack_instantiated(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 0;
    let locals_sig = None;
    let snippet = get_vec_pack(3);
    let snippet_rep = 1;
    let struct_type_params_count = 1;
    let code_inst_signatures = vec![Signature(vec![U8])];
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![0];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![])
}

// Call a vector<T> pack empty and pop with simple generic instantiation
fn vec_pack_gen_simple(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_vec_pack(3);
    let snippet_rep = 1;
    let struct_type_params_count = 5;
    let code_inst_signatures = vec![Signature(vec![Vector(Box::new(TypeParameter(0)))])];
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![0];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![TypeTag::U128])
}

// Call a vector<T> pack empty and pop with deep generic instantiation
fn vec_pack_gen_deep(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    vec_pack_gen_deep_it(addr, session, 1)
}

fn vec_pack_gen_deep_50(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    vec_pack_gen_deep_it(addr, session, 50)
}

fn vec_pack_gen_deep_500(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    vec_pack_gen_deep_it(addr, session, 500)
}

fn vec_pack_gen_deep_it(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
    snippet_rep: usize,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    const STRUCT_TY_PARAMS: usize = 3;
    const STRUCT_TY_ARGS_DEPTH: usize = 2;
    const FUNC_TY_ARGS_DEPTH: usize = 3;

    let mut big_ty = SignatureToken::TypeParameter(0);
    for _ in 0..STRUCT_TY_ARGS_DEPTH {
        let mut ty_args = vec![];
        for _ in 0..STRUCT_TY_PARAMS {
            ty_args.push(big_ty.clone());
        }
        big_ty = StructInstantiation(StructHandleIndex(0), ty_args);
    }

    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_vec_pack(3);
    let struct_type_params_count = STRUCT_TY_PARAMS;
    let code_inst_signatures = vec![Signature(vec![big_ty])];
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    let mut ty_arg = TypeTag::U128;
    for _ in 0..FUNC_TY_ARGS_DEPTH {
        ty_arg = TypeTag::Struct(Box::new(StructTag {
            address: addr,
            module: Identifier::new(MODULE_NAME).unwrap(),
            name: Identifier::new(STRUCT_NAME).unwrap(),
            type_params: vec![ty_arg; STRUCT_TY_PARAMS],
        }));
    }

    //
    // Entry specification
    (self_id, entry_name, vec![ty_arg])
}

// Code for all `VecPack` and `Pop` code
fn get_vec_pack(idx: u16) -> Vec<Bytecode> {
    vec![VecPack(SignatureIndex(idx), 0), Pop]
}

// Call `Exists` on an instantiated generic and pop
fn instantiated_gen_exists(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 0;
    let locals_sig = None;
    let snippet = get_generic_exists();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![Signature(vec![Vector(Box::new(SignatureToken::U8))])];
    let func_handle_idxs = vec![];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![])
}

// Call `Exists` on a simple generic and pop
fn simple_gen_exists(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_exists();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![Signature(vec![StructInstantiation(
        StructHandleIndex(0),
        vec![U64],
    )])];
    let func_handle_idxs = vec![];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![TypeTag::U128])
}

// Call `Exists` on a deep generic and pop
fn deep_gen_exists(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_exists_it(addr, session, 1)
}

fn deep_gen_exists_50(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_exists_it(addr, session, 50)
}

fn deep_gen_exists_500(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_exists_it(addr, session, 500)
}

fn deep_gen_exists_it(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
    snippet_rep: usize,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    const STRUCT_TY_PARAMS: usize = 3;
    const STRUCT_TY_ARGS_DEPTH: usize = 2;
    const FUNC_TY_ARGS_DEPTH: usize = 3;

    let mut big_ty = SignatureToken::TypeParameter(0);
    for _ in 0..STRUCT_TY_ARGS_DEPTH {
        let mut ty_args = vec![];
        for _ in 0..STRUCT_TY_PARAMS {
            ty_args.push(big_ty.clone());
        }
        big_ty = StructInstantiation(StructHandleIndex(0), ty_args);
    }

    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_exists();
    let code_inst_signatures = vec![];
    let struct_type_params_count = STRUCT_TY_PARAMS;
    let struct_inst_signatures = vec![Signature(vec![big_ty; STRUCT_TY_PARAMS])];
    let func_handle_idxs = vec![];
    let func_inst_signatures = vec![];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    let mut ty_arg = TypeTag::U128;
    for _ in 0..FUNC_TY_ARGS_DEPTH {
        ty_arg = TypeTag::Struct(Box::new(StructTag {
            address: addr,
            module: Identifier::new(MODULE_NAME).unwrap(),
            name: Identifier::new(STRUCT_NAME).unwrap(),
            type_params: vec![ty_arg; STRUCT_TY_PARAMS],
        }));
    }

    //
    // Entry specification
    (self_id, entry_name, vec![ty_arg])
}

// Code for all `ExistsGeneric` and `Pop` test
fn get_generic_exists() -> Vec<Bytecode> {
    vec![
        LdConst(ConstantPoolIndex(0)),
        ExistsGeneric(StructDefInstantiationIndex(0)),
        Pop,
    ]
}

// Call an instantiated generic function
fn instantiated_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 0;
    let locals_sig = None;
    let snippet = get_generic_call_loop();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![2];
    let func_inst_signatures = vec![Signature(vec![Vector(Box::new(U64))])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![])
}

// Call simple generic function
fn simple_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_call_loop();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![2];
    let func_inst_signatures = vec![Signature(vec![Vector(Box::new(TypeParameter(0)))])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![TypeTag::U128])
}

// Call deep instantiation generic function
fn deep_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_call_it(addr, session, 1)
}

fn deep_gen_call_50(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_call_it(addr, session, 50)
}

fn deep_gen_call_500(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    deep_gen_call_it(addr, session, 500)
}

fn deep_gen_call_it(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
    snippet_rep: usize,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    const STRUCT_TY_PARAMS: usize = 3;
    const STRUCT_TY_ARGS_DEPTH: usize = 2;
    const FUNC_TY_ARGS_DEPTH: usize = 3;

    let mut big_ty = SignatureToken::TypeParameter(0);
    for _ in 0..STRUCT_TY_ARGS_DEPTH {
        let mut ty_args = vec![];
        for _ in 0..STRUCT_TY_PARAMS {
            ty_args.push(big_ty.clone());
        }
        big_ty = StructInstantiation(StructHandleIndex(0), ty_args);
    }

    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_call_loop();
    let code_inst_signatures = vec![];
    let struct_type_params_count = STRUCT_TY_PARAMS;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![2];
    let func_inst_signatures = vec![Signature(vec![big_ty])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    let mut ty_arg = TypeTag::U128;
    for _ in 0..FUNC_TY_ARGS_DEPTH {
        ty_arg = TypeTag::Struct(Box::new(StructTag {
            address: addr,
            module: Identifier::new(MODULE_NAME).unwrap(),
            name: Identifier::new(STRUCT_NAME).unwrap(),
            type_params: vec![ty_arg; STRUCT_TY_PARAMS],
        }));
    }

    //
    // Entry specification
    (self_id, entry_name, vec![ty_arg])
}

// Code for all `CallGeneric` tests
fn get_generic_call_loop() -> Vec<Bytecode> {
    vec![CallGeneric(FunctionInstantiationIndex(1))]
}

// Call an instantiated generic function
fn instantiated_rec_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 0;
    let locals_sig = None;
    let snippet = get_generic_call();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![1];
    let func_inst_signatures = vec![Signature(vec![Vector(Box::new(U64))])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![])
}

// Call simple generic function
fn simple_rec_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_call();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = 1;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![1];
    let func_inst_signatures = vec![Signature(vec![Vector(Box::new(TypeParameter(0)))])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    //
    // Entry specification
    (self_id, entry_name, vec![TypeTag::U128])
}

// Call deep instantiation generic function
fn deep_rec_gen_call(
    addr: AccountAddress,
    session: &mut Session<&'_ InMemoryStorage>,
) -> (ModuleId, Identifier, Vec<TypeTag>) {
    const STRUCT_TY_PARAMS: usize = 3;
    const STRUCT_TY_ARGS_DEPTH: usize = 2;
    const FUNC_TY_ARGS_DEPTH: usize = 3;

    let mut big_ty = SignatureToken::TypeParameter(0);
    for _ in 0..STRUCT_TY_ARGS_DEPTH {
        let mut ty_args = vec![];
        for _ in 0..STRUCT_TY_PARAMS {
            ty_args.push(big_ty.clone());
        }
        big_ty = StructInstantiation(StructHandleIndex(0), ty_args);
    }

    //
    // Module definition and publishing
    let func_type_params_count = 1;
    let locals_sig = None;
    let snippet = get_generic_call();
    let snippet_rep = 1;
    let code_inst_signatures = vec![];
    let struct_type_params_count = STRUCT_TY_PARAMS;
    let struct_inst_signatures = vec![];
    let func_handle_idxs = vec![1];
    let func_inst_signatures = vec![Signature(vec![big_ty])];

    let (self_id, entry_name) = make_module(
        session,
        addr,
        func_type_params_count,
        locals_sig,
        snippet,
        snippet_rep,
        code_inst_signatures,
        struct_type_params_count,
        struct_inst_signatures,
        func_handle_idxs,
        func_inst_signatures,
    );

    let mut ty_arg = TypeTag::U128;
    for _ in 0..FUNC_TY_ARGS_DEPTH {
        ty_arg = TypeTag::Struct(Box::new(StructTag {
            address: addr,
            module: Identifier::new(MODULE_NAME).unwrap(),
            name: Identifier::new(STRUCT_NAME).unwrap(),
            type_params: vec![ty_arg; STRUCT_TY_PARAMS],
        }));
    }

    //
    // Entry specification
    (self_id, entry_name, vec![ty_arg])
}

// Code for all `CallGeneric` tests
fn get_generic_call() -> Vec<Bytecode> {
    vec![LdU64(0), CallGeneric(FunctionInstantiationIndex(1))]
}
