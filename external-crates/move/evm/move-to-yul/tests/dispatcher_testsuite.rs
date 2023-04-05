// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use anyhow::Result;
use evm::{backend::MemoryVicinity, ExitReason};
use evm_exec_utils::{compile, exec::Executor};
use move_model::{options::ModelBuilderOptions, run_model_builder_with_options};
use move_to_yul::{generator::Generator, options::Options};
use primitive_types::{H160, U256};
use std::path::{Path, PathBuf};

use move_compiler::shared::{NumericalAddress, PackagePaths};
use move_stdlib::move_stdlib_named_addresses;

pub const DISPATCHER_TESTS_LOCATION: &str = "tests/test-dispatcher";

fn path_from_crate_root(path: &str) -> String {
    let mut buf = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    buf.push(path);
    buf.to_string_lossy().to_string()
}

fn contract_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join(DISPATCHER_TESTS_LOCATION)
        .join(file_name)
}

pub fn generate_testing_vincinity() -> MemoryVicinity {
    MemoryVicinity {
        gas_price: 0.into(),
        origin: H160::zero(),
        chain_id: 0.into(),
        block_hashes: vec![],
        block_number: 0.into(),
        block_coinbase: H160::zero(),
        block_timestamp: 0.into(),
        block_difficulty: 0.into(),
        block_gas_limit: U256::MAX,
        block_base_fee_per_gas: 0.into(),
    }
}

fn compile_yul_to_bytecode_bytes(filename: &str) -> Result<Vec<u8>> {
    let deps = vec![
        path_from_crate_root("../stdlib/sources"),
        path_from_crate_root("../../move-stdlib/sources"),
        path_from_crate_root("../../extensions/async/move-async-lib/sources"),
    ];
    let mut named_address_map = move_stdlib_named_addresses();
    named_address_map.insert(
        "std".to_string(),
        NumericalAddress::parse_str("0x1").unwrap(),
    );
    named_address_map.insert(
        "Evm".to_string(),
        NumericalAddress::parse_str("0x2").unwrap(),
    );
    named_address_map.insert(
        "Async".to_string(),
        NumericalAddress::parse_str("0x1").unwrap(),
    );
    let env = run_model_builder_with_options(
        vec![PackagePaths {
            name: None,
            paths: vec![contract_path(filename).to_string_lossy().to_string()],
            named_address_map: named_address_map.clone(),
        }],
        vec![PackagePaths {
            name: None,
            paths: deps,
            named_address_map,
        }],
        ModelBuilderOptions::default(),
    )?;
    let options = Options::default();
    let (_, out, _) = Generator::run(&options, &env)
        .pop()
        .expect("not contract in test case");
    let (bc, _) = compile::solc_yul(&out, false)?;
    Ok(bc)
}

/// Test DispatcherBasic
#[test]
fn test_dispatch_basic() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherBasic.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");
    for i in 0..3 {
        let sig = format!("return_{}()", i);
        let (exit_reason, buffer) =
            exec.call_function(H160::zero(), contract_address, 0.into(), &sig, &[]);
        assert!(matches!(exit_reason, ExitReason::Succeed(_)));
        assert!(buffer.len() >= 32);
        let mut expected = [0u8; 32];
        expected[31] = i;
        assert_eq!(&buffer[..32], &expected);
    }
    Ok(())
}

/// Test DispatcherBasicStorage
#[test]
fn test_dispatch_basic_storage() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherBasicStorage.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");
    let current = "current()";
    let increment = "increment()";
    for i in 0..3 {
        let (exit_reason, buffer) =
            exec.call_function(H160::zero(), contract_address, 0.into(), current, &[]);
        assert!(matches!(exit_reason, ExitReason::Succeed(_)));
        assert!(buffer.len() >= 32);
        assert_eq!(&buffer[31], &(i as u8));
        let (exit_reason, _) =
            exec.call_function(H160::zero(), contract_address, 0.into(), increment, &[]);
        assert!(matches!(exit_reason, ExitReason::Succeed(_)));
    }
    Ok(())
}

/// Test DispatcherRevert
#[test]
fn test_dispatch_revert() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherRevert.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");
    let sig = "return_1";
    let (exit_reason, _) = exec.call_function(H160::zero(), contract_address, 0.into(), sig, &[]);
    assert!(matches!(exit_reason, ExitReason::Revert(_)));
    Ok(())
}

/// Test DispatcherFallback
#[test]
fn test_dispatch_fallback() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherFallback.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");
    let sig = "return_1";
    let (exit_reason, _) = exec.call_function(H160::zero(), contract_address, 0.into(), sig, &[]);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));
    Ok(())
}

/// Test decoding array types (u8)
#[test]
fn test_dispatch_array_decoding_u8() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u8(uint8[2])";
    let mut args = [0u8; 64];
    args[31] = 42; // arr[0]
    args[63] = 24; // arr[1]

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 32];
    expected[31] = args[31] + args[63];
    assert_eq!(&buffer[..32], &expected);

    //////
    let sig = "test_u8(uint8[2][2])";
    let mut args = [0u8; 128];
    args[31] = 1; // arr[0][0]
    args[63] = 2; // arr[1][0]
    args[95] = 3; // arr[0][1]
    args[127] = 4; // arr[1][1]

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 32];
    expected[31] = args[31] + args[63] + args[95] - args[127];
    assert_eq!(&buffer[..32], &expected);

    //////
    let sig = "test_u8(uint8[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to arr
    args[63] = 2; //  size of arr
    args[95] = 5; //  arr_1
    args[127] = 6;
    args[159] = 7; // arr_2
    args[191] = 8;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = args[95] + args[127] + args[159] + args[191];
    assert_eq!(&buffer[..32], &expected);

    //////
    let sig = "test_u8(uint8[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to array
    args[63] = 64; // pointer to array 1
    args[95] = 160; // pointer to array 2
    args[127] = 2; // length of array 1
    args[159] = 9;
    args[191] = 10;
    args[223] = 3; // length of array 2
    args[255] = 11;
    args[287] = 12;
    args[319] = 13;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = args[159] + args[191] + args[255] + args[287] + args[319];
    assert_eq!(&buffer, &expected);

    Ok(())
}

/// Test decoding array types (u64)
#[test]
fn test_dispatch_array_decoding_u64() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u64(uint64[2])";
    let mut args = [0u8; 64];
    for item in args.iter_mut().take(32).skip(24) {
        *item = 11; // arr[0]
    }
    for item in args.iter_mut().skip(56) {
        *item = 22; // arr[1]
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(24) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(24) {
        assert_eq!(*item, 33);
    }

    //////
    let sig = "test_u64(uint64[2][2])";
    let mut args = [0u8; 128];
    for item in args.iter_mut().take(32).skip(24) {
        *item = 11; // arr[0][0]
    }
    for item in args.iter_mut().take(64).skip(56) {
        *item = 11; // arr[1][0]
    }
    for item in args.iter_mut().take(96).skip(88) {
        *item = 11; // arr[0][1]
    }
    for item in args.iter_mut().skip(120) {
        *item = 11; // arr[1][1]
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(24) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(24) {
        assert_eq!(*item, 44);
    }

    //////
    let sig = "test_u64(uint64[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to the array
    args[63] = 2; // size of the dynamic array
    for item in args.iter_mut().take(96).skip(88) {
        *item = 11; // arr[0][0]
    }
    for item in args.iter_mut().take(128).skip(120) {
        *item = 11; // arr[1][0]
    }
    for item in args.iter_mut().take(160).skip(152) {
        *item = 11; // arr[0][1]
    }
    for item in args.iter_mut().skip(184) {
        *item = 11; // arr[1][1]
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(24) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(24) {
        assert_eq!(*item, 44);
    }

    //////
    let sig = "test_u64(uint64[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to array
    args[63] = 64; // pointer to arr_1
    args[95] = 160; // pointer to arr_2
    args[127] = 2; // length of arr_1
    for item in args.iter_mut().take(160).skip(152) {
        *item = 11; // arr_1[0][0]
    }
    for item in args.iter_mut().take(192).skip(184) {
        *item = 11; // arr_1[1][0]
    }
    args[223] = 3; // length of arr_2
    for item in args.iter_mut().take(256).skip(248) {
        *item = 11; // arr_2[0][1]
    }
    for item in args.iter_mut().take(288).skip(280) {
        *item = 11; // arr_2[1][1]
    }
    for item in args.iter_mut().skip(312) {
        *item = 11; // arr_2[2][1]
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(24) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(24) {
        assert_eq!(*item, 55);
    }

    Ok(())
}

/// Test decoding array types (u128)
#[test]
fn test_dispatch_array_decoding_u128() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u128(uint128[][])";
    let mut args = [0u8; 416];
    args[31] = 32; // pointer to arr
    args[63] = 2; // length of arr
    args[95] = 64; // pointer to arr_1
    args[127] = 192; // pointer to arr_2
    args[159] = 3; // length of arr_1
    for i in 16..32 {
        args[160 + i] = 11;
        args[160 + 32 + i] = 22;
        args[160 + 64 + i] = 11;
    }
    args[287] = 4; // length of arr_2
    for i in 16..32 {
        args[288 + i] = 11;
        args[288 + 32 + i] = 22;
        args[288 + 64 + i] = 11;
        args[288 + 96 + i] = 11;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() >= 32);
    for item in buffer.iter().take(16) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(16) {
        assert_eq!(*item, 99);
    }

    Ok(())
}

/// Test decoding array types (U256)
#[test]
fn test_dispatch_array_decoding_u256() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u256(uint256[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to arr
    args[63] = 2; // size of arr
    for item in args.iter_mut().skip(64) {
        *item = 11;
    }
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(32) {
        assert_eq!(*item, 44);
    }

    //////
    let sig = "test_u256(uint256[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to arr
    args[63] = 64; // pointer to arr_1
    args[95] = 160; // pointer to arr_2
    args[127] = 2; // length of arr_1
    for item in args.iter_mut().take(192).skip(128) {
        *item = 11; // arr_1
    }
    args[223] = 3; // length of arr_2
    for item in args.iter_mut().skip(224) {
        *item = 11; // arr_2
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(32) {
        assert_eq!(*item, 55);
    }

    Ok(())
}

/// Test decoding array types
#[test]
fn test_dispatch_array_decoding_type_compatibility() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_uint8_u64(uint8[2])";
    let mut args = [0u8; 64];
    args[31] = 42;
    args[63] = 24;
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() >= 32);
    let mut expected = [0u8; 32];
    expected[31] = args[31] + args[63];
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_uint16_u64(uint16[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to arr
    args[63] = 2; // size of arr
    args[94] = 1; // arr_1
    args[95] = 5;
    args[126] = 1;
    args[127] = 6;
    args[158] = 1; // arr_2
    args[159] = 7;
    args[190] = 1;
    args[191] = 8;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[30] = args[94] + args[126] + args[158] + args[190];
    expected[31] = args[95] + args[127] + args[159] + args[191];
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_uint72_u128(uint72[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to arr
    args[63] = 64; // pointer to arr_1
    args[95] = 160; // pointer to arr_2
    args[127] = 2; // length of arr_1
    for item in args.iter_mut().take(160).skip(151) {
        *item = 11;
    }
    for item in args.iter_mut().take(192).skip(183) {
        *item = 11;
    }
    args[223] = 3; // length of arr_2
    for item in args.iter_mut().take(256).skip(247) {
        *item = 11;
    }
    for item in args.iter_mut().take(288).skip(279) {
        *item = 11;
    }
    for item in args.iter_mut().skip(311) {
        *item = 11;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    for item in buffer.iter().take(23) {
        assert_eq!(*item, 0);
    }
    for item in buffer.iter().take(32).skip(23) {
        assert_eq!(*item, 55);
    }

    //////
    let sig = "test_uint8_U256(uint8[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to arr
    args[63] = 2; // size of arr
    args[95] = 5; // arr_1
    args[127] = 6;
    args[159] = 7; // arr_2
    args[191] = 8;
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = args[95] + args[127] + args[159] + args[191];
    assert_eq!(&buffer, &expected);

    Ok(())
}

/// Test decoding bytes
#[test]
fn test_dispatch_bytes_decoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherBytesDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_static_bytes_len(bytes4)";
    let mut args = [0u8; 32];
    args[0] = 1;
    args[1] = 2;
    args[2] = 3;
    args[3] = 4;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 4; // length of bytes4
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_static_bytes_last_elem(bytes32)";
    let mut args = [0u8; 32];
    for i in 1..33 {
        args[i - 1] = i as u8;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 32; // last element of bytes32
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_dynamic_bytes_len(bytes)";
    let mut args = [0u8; 128];
    args[31] = 32;
    args[63] = 33;
    for (i, item) in args.iter_mut().enumerate().take(97).skip(64) {
        *item = i as u8;
    }
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 33;
    assert_eq!(&buffer[..32], &expected);

    //////
    let sig = "test_dynamic_bytes_last_elem(bytes)";
    let mut args = [0u8; 128];
    args[31] = 32;
    args[63] = 33;
    for (i, item) in args.iter_mut().enumerate().take(97).skip(64) {
        *item = i as u8;
    }
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 96; // last element of bytes
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_bytes5_2_dynamic_size_sum(bytes5[2][])";
    let mut args = [0u8; 256];
    args[31] = 32; // pointer to arr
    args[63] = 3; // length of arr
    for i in 0..5 {
        // construct bytes5
        args[64 + i] = 1;
        args[64 + 32 + i] = 2;
        args[64 + 64 + i] = 3;
        args[64 + 96 + i] = 4;
        args[64 + 128 + i] = 5;
        args[64 + 160 + i] = 6;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 64);
    let mut expected = [0u8; 64];
    expected[31] = 30;
    expected[63] = 21;
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_string(string)";
    let mut args = [0u8; 96];
    args[31] = 32;
    args[63] = 5;
    args[64] = 72;
    args[65] = 101;
    args[66] = 108;
    args[67] = 108;
    args[68] = 111;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 64);
    let mut expected = [0u8; 64];
    expected[31] = 5; // length
    expected[63] = 111; // last byte of the string
    assert_eq!(&buffer, &expected);

    Ok(())
}

/// Test decoding multiple parameters
#[test]
fn test_dispatch_multi_paras_decoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherMultiParaDecoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u8_array_2_uint64(uint8[2],uint64)";
    let mut args = [0u8; 96];
    args[31] = 41; // arr[0]
    args[63] = 42; // arr[1]
    args[95] = 1;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 42; // arr[1]
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_uint64_u8_array_3_uint64(uint64,uint8[3],uint64)";
    let mut args = [0u8; 160];
    args[31] = 0;
    args[63] = 43;
    args[95] = 44;
    args[127] = 45;
    args[159] = 2;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 64);
    let mut expected = [0u8; 64];
    expected[31] = 43; // arr[0]
    expected[63] = 45; // arr[2]
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_u8_array_uint64_u8_array_uint64(uint8[2],uint64,uint8[2],uint64)";
    let mut args = [0u8; 192];
    args[31] = 41;
    args[63] = 42;
    args[95] = 0;
    args[127] = 43;
    args[159] = 44;
    args[191] = 1;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 85; // arr[0] + arr[1]
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_string_length_sum(string,string)";
    let mut args = [0u8; 32 + 32 + 32 + 64 + 32 + 32];
    args[31] = 64; // pointer to str_1
    args[63] = 160; // pointer to str_2
    args[95] = 34; // length of str_1
    for (i, item) in args.iter_mut().enumerate().take(130).skip(96) {
        *item = (i - 41) as u8;
    }
    args[191] = 5; // length of str_2
    for (i, item) in args.iter_mut().enumerate().take(197).skip(192) {
        *item = (197 - i) as u8;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    expected[31] = 39; // str_1.len() + str_2.len()
    assert_eq!(&buffer[..32], &expected);

    //////
    let sig = "test_static_array_string_static_array_string(string,uint8[2],string,uint8[2])";
    let mut args = [0u8; 32 + 64 + 32 + 64 + 32 + 32 + 32 + 32];
    args[31] = 192; // pointer to str_1
    args[63] = 1; // arr_1[0]
    args[95] = 2; // arr_1[1]
    args[126] = 1; // pointer to str_2
    args[127] = 0; // pointer to str_2 (256)
    args[159] = 3; // arr_2[0]
    args[191] = 4; // arr_2[1]
    args[223] = 3; // length of str1
    for item in args.iter_mut().take(227).skip(224) {
        *item = 6;
    }
    args[287] = 5; // length of str2
    for item in args.iter_mut().take(294).skip(288) {
        *item = 7;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 64);
    let mut expected = [0u8; 64];
    expected[31] = 12; // sum(str_1.len + str_2.len + arr_1.len + arr_2.len)
    expected[63] = 19; // arr_1[1] + arr_2[1] + str_1[2] + str_1[3] = 2 + 4 + 6 + 7
    assert_eq!(&buffer[..64], &expected);

    Ok(())
}

/// Test encoding array types
#[test]
fn test_dispatch_array_encoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherArrayEncoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u8(uint8[2])";
    let mut args = [0u8; 64];
    args[31] = 42;
    args[63] = 24;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_u8(uint8[2][2])";
    let mut args = [0u8; 128];
    args[31] = 1;
    args[63] = 2;
    args[95] = 3;
    args[127] = 4;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_u8(uint8[2][])";
    let mut args = [0u8; 192];
    args[31] = 32; // pointer to arr
    args[63] = 2; // size of arr
    args[95] = 5; // arr_1
    args[127] = 6;
    args[159] = 7; // arr_2
    args[191] = 8;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////////
    let sig = "test_u8(uint8[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to array
    args[63] = 64; // pointer to array 1
    args[95] = 160; // pointer to array 2
    args[127] = 2; // first array length
    args[159] = 9;
    args[191] = 10;
    args[223] = 3; // second array length
    args[255] = 11;
    args[287] = 12;
    args[319] = 13;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() >= 320);
    assert_eq!(&buffer, &args);

    //////
    let sig = "test_u64(uint64[2])";
    let mut args = [0u8; 64];
    for item in args.iter_mut().take(32).skip(24) {
        *item = 11;
    }
    for item in args.iter_mut().skip(56) {
        *item = 22;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_u128(uint128[][])";
    let mut args = [0u8; 416];
    args[31] = 32; // pointer to arr
    args[63] = 2; // length of arr
    args[95] = 64; // pointer to arr_1
    args[127] = 192; // pointer to arr_2
    args[159] = 3; // length of arr_1
    for i in 16..32 {
        args[160 + i] = 11;
        args[160 + 32 + i] = 22;
        args[160 + 64 + i] = 11;
    }
    args[287] = 4; // length of arr_2
    for i in 16..32 {
        args[288 + i] = 11;
        args[288 + 32 + i] = 22;
        args[288 + 64 + i] = 11;
        args[288 + 96 + i] = 11;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_u256(uint256[][2])";
    let mut args = [0u8; 320];

    args[31] = 32; // pointer to arr
    args[63] = 64; // pointer to arr_1
    args[95] = 160; // pointer to arr_2
    args[127] = 2; // length of arr_1
    for item in args.iter_mut().take(192).skip(128) {
        *item = 11; // arr_1
    }
    args[223] = 3; // length of arr_2
    for item in args.iter_mut().skip(224) {
        *item = 11; // arr_2
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_uint72_u128(uint72[][2])";
    let mut args = [0u8; 320];
    args[31] = 32; // pointer to arr
    args[63] = 64; // pointer to arr_1
    args[95] = 160; // pointer to arr_2
    args[127] = 2; // length of arr_1
    for item in args.iter_mut().take(160).skip(151) {
        *item = 11;
    }
    for item in args.iter_mut().take(192).skip(183) {
        *item = 11;
    }
    args[223] = 3; // length of arr_2
    for item in args.iter_mut().take(256).skip(247) {
        *item = 11;
    }
    for item in args.iter_mut().take(288).skip(279) {
        *item = 11;
    }
    for item in args.iter_mut().skip(311) {
        *item = 11;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    Ok(())
}

/// Test encoding bytes
#[test]
fn test_dispatch_bytes_encoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherBytesEncoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_static_bytes(bytes4,uint8)";
    let mut args = [0u8; 64];
    args[0] = 1;
    args[1] = 2;
    args[2] = 3;
    args[3] = 4;
    args[63] = 42;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() == 32);
    let mut expected = [0u8; 32];
    for (i, item) in expected.iter_mut().enumerate().take(4) {
        *item = (i + 1) as u8;
    }
    expected[4] = args[63];

    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_bytes5_2_dynamic(bytes5[2][])";
    let mut args = [0u8; 256];
    args[31] = 32; // pointer to arr
    args[63] = 3; // length of arr
    for i in 0..5 {
        // construct three bytes5 pair
        args[64 + i] = 1;
        args[64 + 32 + i] = 2;
        args[64 + 64 + i] = 3;
        args[64 + 96 + i] = 4;
        args[64 + 128 + i] = 5;
        args[64 + 160 + i] = 6;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_bytes(bytes)";
    let mut args = [0u8; 128];
    args[31] = 32; // pointer to bytes
    args[63] = 33; // length of bytes
    for (i, item) in args.iter_mut().enumerate().take(97).skip(64) {
        *item = i as u8;
    }
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    //////
    let sig = "test_string(string)";
    let mut args = [0u8; 96];
    args[31] = 32; // pointer to str
    args[63] = 5; // length of str
    args[64] = 72;
    args[65] = 101;
    args[66] = 108;
    args[67] = 108;
    args[68] = 111;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(&buffer, &args);

    Ok(())
}

/// Test encoding multiple return values
#[test]
fn test_dispatch_multi_ret_encoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherMultiRetEncoding.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_u8_array_2_uint64(uint8[2],uint64)";
    let mut args = [0u8; 96];
    args[31] = 41; //
    args[63] = 42;
    args[95] = 1;
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 96];
    expected[31] = 42; // arr[1]
    expected[63] = 41; // arr
    expected[95] = 42;
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_uint64_u8_array_3_uint64(uint64,uint8[3],uint64)";
    let mut args = [0u8; 160];
    args[31] = 0;
    args[63] = 43;
    args[95] = 44;
    args[127] = 45;
    args[159] = 2;
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 160];
    expected[31] = 43; // arr[0]
    expected[63] = 43; // arr
    expected[95] = 44;
    expected[127] = 45;
    expected[159] = 45; // arr[2]
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_two_strings(string,string)";
    let mut args = [0u8; 32 + 32 + 32 + 64 + 32 + 32];
    args[31] = 64; // pointer to str_1
    args[63] = 160; // pointer to str_2
    args[95] = 34; // length of str_1
    for (i, item) in args.iter_mut().enumerate().take(130).skip(96) {
        // str_1
        *item = (i - 41) as u8;
    }
    args[191] = 5; // pointer to str_2
    for (i, item) in args.iter_mut().enumerate().take(197).skip(192) {
        // str_2
        *item = (197 - i) as u8;
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 224];
    expected[31] = 64; // pointer to str_2
    expected[63] = 128; // pointer to str_1
    expected[95] = 5; // length of str_2
    for (i, item) in expected.iter_mut().enumerate().take(101).skip(96) {
        // str_2
        *item = (197 - i - 96) as u8;
    }
    expected[159] = 34; // length of str_1
    for (i, item) in expected.iter_mut().enumerate().take(194).skip(160) {
        // str_1
        *item = (i - 64 - 41) as u8;
    }
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_string_uint8(string)";
    let mut args = [0u8; 96];
    args[31] = 32; // pointer to str
    args[63] = 5; // length of str
    args[64] = 72;
    args[65] = 101;
    args[66] = 108;
    args[67] = 108;
    args[68] = 111;

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &args);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert_eq!(buffer[31], 64); // pointer to str
    assert_eq!(&buffer[64..128], &args[32..]); // str
    assert_eq!(buffer[63], 111); // last byte of str

    Ok(())
}

/// Test encoding string from storage
#[test]
fn test_storage_string_encoding() -> Result<()> {
    let contract_code = compile_yul_to_bytecode_bytes("DispatcherEncodingStorage.move")?;
    let vicinity = generate_testing_vincinity();
    let mut exec = Executor::new(&vicinity);
    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    //////
    let sig = "test_string()";
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &[]);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 96];
    expected[31] = 32;
    expected[63] = 1;
    expected[64] = 65;
    assert_eq!(&buffer, &expected);

    //////
    let sig = "test_vec_u64()";
    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), sig, &[]);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let mut expected = [0u8; 224];
    expected[31] = 64;
    expected[63] = 128;
    expected[95] = 1;
    expected[96] = 65;
    expected[159] = 2;
    expected[191] = 64;
    expected[223] = 65;

    assert_eq!(&buffer, &expected);

    Ok(())
}
