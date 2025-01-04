// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::{collections::VecDeque, convert::TryFrom};
use sui_types::base_types::{ObjectID, TransactionDigest};

use crate::{object_runtime::ObjectRuntime, NativesCostTable};

#[derive(Clone)]
pub struct TxContextDeriveIdCostParams {
    pub tx_context_derive_id_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun derive_id
 * Implementation of the Move native function `fun derive_id(tx_hash: vector<u8>, ids_created: u64): address`
 *   gas cost: tx_context_derive_id_cost_base                | we operate on fixed size data structures
 **************************************************************************************************/
pub fn derive_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 2);

    let tx_context_derive_id_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()
        .tx_context_derive_id_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_derive_id_cost_params.tx_context_derive_id_cost_base
    );

    let ids_created = pop_arg!(args, u64);
    let tx_hash = pop_arg!(args, Vec<u8>);

    // unwrap safe because all digests in Move are serialized from the Rust `TransactionDigest`
    let digest = TransactionDigest::try_from(tx_hash.as_slice()).unwrap();
    let address = AccountAddress::from(ObjectID::derive_id(digest, ids_created));
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut();
    obj_runtime.new_id(address.into())?;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(address)],
    ))
}

#[derive(Clone)]
pub struct TxContextNativeSenderCostParams {
    pub tx_context_native_sender_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_sender(): address;
 **************************************************************************************************/
pub fn native_sender(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424242))
}

#[derive(Clone)]
pub struct TxContextNativeDigestCostParams {
    pub tx_context_native_digest_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_digest(): &vector<u8>;
 **************************************************************************************************/
pub fn native_digest(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424243))
}

#[derive(Clone)]
pub struct TxContextNativeEpochCostParams {
    pub tx_context_native_epoch_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_epoch(): u64;
 **************************************************************************************************/
pub fn native_epoch(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424244))
}

#[derive(Clone)]
pub struct TxContextNativeEpochTimestampMsCostParams {
    pub tx_context_native_epoch_timestamp_ms_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_epoch_timestamp_ms(): u64;
 **************************************************************************************************/
pub fn native_epoch_timestamp_ms(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424245))
}

#[derive(Clone)]
pub struct TxContextNativeSponsorCostParams {
    pub tx_context_native_sponsor_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_sponsor(): address;
 **************************************************************************************************/
pub fn native_sponsor(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424246))
}

#[derive(Clone)]
pub struct TxContextNativeIdsCreatedCostParams {
    pub tx_context_native_ids_created_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_ids_created(): u64;
 **************************************************************************************************/
pub fn native_ids_created(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424247))
}

#[derive(Clone)]
pub struct TxContextNativeIncIdsCreatedCostParams {
    pub tx_context_native_inc_ids_created_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_inc_ids_created();
 **************************************************************************************************/
pub fn native_inc_ids_created(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424248))
}

#[derive(Clone)]
pub struct TxContextNativeIncEpochCostParams {
    pub tx_context_native_inc_epoch_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_inc_epoch();
 **************************************************************************************************/
pub fn native_inc_epoch(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.is_empty());

    Ok(NativeResult::err(0.into(), 424249))
}

#[derive(Clone)]
pub struct TxContextNativeIncEpochTimestampCostParams {
    pub tx_context_native_inc_epoch_timestamp_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_inc_epoch_timestamp();
 **************************************************************************************************/
pub fn native_inc_epoch_timestamp(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.len() == 1);

    Ok(NativeResult::err(0.into(), 424250))
}

#[derive(Clone)]
pub struct TxContextNativeReplaceCostParams {
    pub tx_context_native_replace_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_replace();
 **************************************************************************************************/
pub fn native_replace(
    _context: &mut NativeContext,
    _ty_args: Vec<Type>,
    _args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(_ty_args.is_empty());
    debug_assert!(_args.len() == 5);

    Ok(NativeResult::err(0.into(), 424251))
}
