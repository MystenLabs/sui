// Copyright (c) The Diem Core Contributors
// Copyright (c) The Move Contributors
// SPDX-License-Identifier: Apache-2.0

use crate::{compile::solc, exec::Executor};
use anyhow::Result;
use evm::{backend::MemoryVicinity, ExitReason, Opcode};
use primitive_types::{H160, U256};
use std::path::{Path, PathBuf};

fn contract_path(file_name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("contracts")
        .join(file_name)
}

fn test_vincinity() -> MemoryVicinity {
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

#[test]
fn test_hello_world() -> Result<()> {
    let (_contract_name, contract_code) = solc([contract_path("hello_world.sol")])?
        .into_iter()
        .next()
        .unwrap();

    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let res = exec.create_contract(H160::zero(), contract_code);
    assert!(res.is_ok());

    Ok(())
}

#[test]
fn test_two_functions() -> Result<()> {
    let (_contract_name, contract_code) = solc([contract_path("two_functions.sol")])?
        .into_iter()
        .next()
        .unwrap();

    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    let (exit_reason, _buffer) = exec.call_function(
        H160::zero(),
        contract_address,
        0.into(),
        "do_nothing()",
        &[],
    );
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    let (exit_reason, _buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), "panic()", &[]);
    assert!(matches!(exit_reason, ExitReason::Revert(_)));

    Ok(())
}

#[test]
fn test_a_plus_b() -> Result<()> {
    let (_contract_name, contract_code) = solc([contract_path("a_plus_b.sol")])?
        .into_iter()
        .next()
        .unwrap();

    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    let mut args = [0u8; 64];
    args[0] = 1;
    args[32] = 2;

    let (exit_reason, buffer) = exec.call_function(
        H160::zero(),
        contract_address,
        0.into(),
        "plus(uint256,uint256)",
        &args,
    );
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() >= 32);
    let mut expected = [0u8; 32];
    expected[0] = 0x3;
    assert_eq!(&buffer[..32], &expected);

    Ok(())
}

#[test]
fn test_stateful() -> Result<()> {
    let (_contract_name, contract_code) = solc([contract_path("stateful.sol")])?
        .into_iter()
        .next()
        .unwrap();

    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let contract_address = exec
        .create_contract(H160::zero(), contract_code)
        .expect("failed to create contract");

    for _ in 0..7 {
        let (exit_reason, _buffer) =
            exec.call_function(H160::zero(), contract_address, 0.into(), "inc()", &[]);
        assert!(matches!(exit_reason, ExitReason::Succeed(_)));
    }

    let (exit_reason, buffer) =
        exec.call_function(H160::zero(), contract_address, 0.into(), "get()", &[]);
    assert!(matches!(exit_reason, ExitReason::Succeed(_)));

    assert!(buffer.len() >= 32);
    let mut expected = [0u8; 32];
    expected[31] = 0x7;
    assert_eq!(&buffer[..32], &expected);

    Ok(())
}

#[test]
fn test_custom_code_add_two_numbers() -> Result<()> {
    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let mut code = vec![];

    // push 1
    // push 2
    // add
    // push 0
    // mstore
    // push 32
    // push 0
    // return

    code.extend([Opcode::PUSH1.0, 0x1, Opcode::PUSH1.0, 0x2, Opcode::ADD.0]);

    code.push(Opcode::PUSH32.0);
    code.extend([0; 32]);
    code.push(Opcode::MSTORE.0);

    let mut length = [0; 32];
    length[31] = 32;
    code.push(Opcode::PUSH32.0);
    code.extend(length);
    code.push(Opcode::PUSH32.0);
    code.extend([0; 32]);
    code.push(Opcode::RETURN.0);

    let ret = exec.execute_custom_code(H160::zero(), H160::zero(), code, vec![]);

    let mut expected = [0; 32];
    expected[31] = 0x3;
    assert_eq!(&ret.return_value, &expected);

    Ok(())
}

#[test]
fn test_transfer_eth() -> Result<()> {
    let vicinity = test_vincinity();
    let mut exec = Executor::new(&vicinity);

    let alice_addr = H160::from([0x1; 20]);
    let bob_addr = H160::from([0x2; 20]);

    exec.mint(alice_addr, U256::from(5000))?;
    exec.mint(bob_addr, U256::from(5000))?;

    assert_eq!(exec.account_balance(alice_addr).unwrap(), U256::from(5000));
    assert_eq!(exec.account_balance(bob_addr).unwrap(), U256::from(5000));

    exec.transfer_eth(alice_addr, bob_addr, U256::from(1000))
        .expect("failed to transfer eth");

    assert_eq!(exec.account_balance(alice_addr).unwrap(), U256::from(4000));
    assert_eq!(exec.account_balance(bob_addr).unwrap(), U256::from(6000));

    Ok(())
}
