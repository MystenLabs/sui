// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0
use crate::NativesCostTable;
use fastcrypto::hash::{Blake2b256, HashFunction, Keccak256};
use fastcrypto_zkp::bn254::poseidon::hash_to_bytes;
use move_binary_format::errors::PartialVMResult;
use move_core_types::gas_algebra::InternalGas;
use move_core_types::u256::U256;
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type,
    natives::function::NativeResult,
    pop_arg,
    values::{Value, VectorRef},
};
use smallvec::smallvec;
use std::{collections::VecDeque, ops::Mul};

const BLAKE_2B256_BLOCK_SIZE: u16 = 128;
const KECCAK_256_BLOCK_SIZE: u16 = 136;

fn hash<H: HashFunction<DIGEST_SIZE>, const DIGEST_SIZE: usize>(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
    // The caller provides the cost per byte
    msg_cost_per_byte: InternalGas,
    // The caller provides the cost per block
    msg_cost_per_block: InternalGas,
    // The caller specifies the block size
    block_size: u16,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    let msg = pop_arg!(args, VectorRef);
    let msg_ref = msg.as_bytes_ref();

    let block_size = block_size as usize;

    // Charge the msg dependent costs
    native_charge_gas_early_exit!(
        context,
        msg_cost_per_byte.mul((msg_ref.len() as u64).into())
            // Round up the blocks
            + msg_cost_per_block
                .mul((((msg_ref.len() + block_size - 1) / block_size) as u64).into())
    );

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::vector_u8(
            H::digest(msg.as_bytes_ref().as_slice()).digest
        )],
    ))
}

#[derive(Clone)]
pub struct HashKeccak256CostParams {
    /// Base cost for invoking the `blake2b256` function
    pub hash_keccak256_cost_base: InternalGas,
    /// Cost per byte of `data`
    pub hash_keccak256_data_cost_per_byte: InternalGas,
    /// Cost per block of `data`, where a block is 136 bytes
    pub hash_keccak256_data_cost_per_block: InternalGas,
}

/***************************************************************************************************
 * native fun keccak256
 * Implementation of the Move native function `hash::keccak256(data: &vector<u8>): vector<u8>`
 *   gas cost: hash_keccak256_cost_base                               | base cost for function call and fixed opers
 *              + hash_keccak256_data_cost_per_byte * msg.len()       | cost depends on length of message
 *              + hash_keccak256_data_cost_per_block * num_blocks     | cost depends on number of blocks in message
 **************************************************************************************************/
pub fn keccak256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Load the cost parameters from the protocol config
    let hash_keccak256_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .hash_keccak256_cost_params
        .clone();
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(context, hash_keccak256_cost_params.hash_keccak256_cost_base);

    hash::<Keccak256, 32>(
        context,
        ty_args,
        args,
        hash_keccak256_cost_params.hash_keccak256_data_cost_per_byte,
        hash_keccak256_cost_params.hash_keccak256_data_cost_per_block,
        KECCAK_256_BLOCK_SIZE,
    )
}

#[derive(Clone)]
pub struct HashBlake2b256CostParams {
    /// Base cost for invoking the `blake2b256` function
    pub hash_blake2b256_cost_base: InternalGas,
    /// Cost per byte of `data`
    pub hash_blake2b256_data_cost_per_byte: InternalGas,
    /// Cost per block of `data`, where a block is 128 bytes
    pub hash_blake2b256_data_cost_per_block: InternalGas,
}
/***************************************************************************************************
 * native fun blake2b256
 * Implementation of the Move native function `hash::blake2b256(data: &vector<u8>): vector<u8>`
 *   gas cost: hash_blake2b256_cost_base                               | base cost for function call and fixed opers
 *              + hash_blake2b256_data_cost_per_byte * msg.len()       | cost depends on length of message
 *              + hash_blake2b256_data_cost_per_block * num_blocks     | cost depends on number of blocks in message
 **************************************************************************************************/
pub fn blake2b256(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Load the cost parameters from the protocol config
    let hash_blake2b256_cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .hash_blake2b256_cost_params
        .clone();
    // Charge the base cost for this oper
    native_charge_gas_early_exit!(
        context,
        hash_blake2b256_cost_params.hash_blake2b256_cost_base
    );

    hash::<Blake2b256, 32>(
        context,
        ty_args,
        args,
        hash_blake2b256_cost_params.hash_blake2b256_data_cost_per_byte,
        hash_blake2b256_cost_params.hash_blake2b256_data_cost_per_block,
        BLAKE_2B256_BLOCK_SIZE,
    )
}

pub const INVALID_POSEIDON_INPUTS: u64 = 0;

#[derive(Clone)]
pub struct HashPoseidonBN254CostParams {
    /// Base cost for invoking the `poseidon_bn254` function
    pub hash_poseidon_bn254_cost_base: InternalGas,
    /// Cost per block of `data`, where a block is 32 bytes
    pub hash_poseidon_bn254_data_cost_per_block: InternalGas,
}

/***************************************************************************************************
 * native fun poseidon_bn254
 * Implementation of the Move native function `hash::poseidon_bn254(data: &vector<u8>): vector<u8>`
 *   gas cost: hash_poseidon_bn254_cost_base                           | base cost for function call and fixed opers
 *              + hash_poseidon_bn254_data_cost_per_block * num_blocks | cost depends on number of blocks in message
 **************************************************************************************************/
pub fn poseidon_bn254(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    // Load the cost parameters from the protocol config
    let cost_params = &context
        .extensions()
        .get::<NativesCostTable>()
        .hash_poseidon_bn254_cost_params
        .clone();

    // Charge the base cost for this operation
    native_charge_gas_early_exit!(context, cost_params.hash_poseidon_bn254_cost_base);

    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 1);

    // The input is a reference to a vector of vector<u8>'s
    let inputs = pop_arg!(args, VectorRef);
    let length = inputs.len(&Type::U256)?.value_as::<u64>()?;

    let mut field_elements: Vec<Vec<u8>> = Vec::new();

    for _ in 0..length {
        let input = inputs.pop(&Type::U256)?.value_as::<U256>()?;
        field_elements.push(input.to_le_bytes().to_vec());
    }

    // Inputs are read in reverse order
    field_elements.reverse();

    match hash_to_bytes(&field_elements) {
        Ok(hash) => {
            let result = U256::from_le_bytes(&hash);
            Ok(NativeResult::ok(
                context.gas_used(),
                smallvec![Value::u256(result)],
            ))
        }
        Err(_) => Ok(NativeResult::err(
            context.gas_used(),
            INVALID_POSEIDON_INPUTS,
        )),
    }
}
