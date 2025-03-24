// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use move_binary_format::errors::PartialVMResult;
use move_core_types::{account_address::AccountAddress, gas_algebra::InternalGas};
use move_vm_runtime::{native_charge_gas_early_exit, native_functions::NativeContext};
use move_vm_types::{
    loaded_data::runtime_types::Type, natives::function::NativeResult, pop_arg, values::Value,
};
use smallvec::smallvec;
use std::collections::VecDeque;
use sui_types::{base_types::ObjectID, digests::TransactionDigest};

use crate::{
    object_runtime::ObjectRuntime, transaction_context::TransactionContext, NativesCostTable,
};

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
        .get::<NativesCostTable>()?
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
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;
    obj_runtime.new_id(address.into())?;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(address)],
    ))
}
#[derive(Clone)]
pub struct TxContextFreshIdCostParams {
    pub tx_context_fresh_id_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun fresh_id
 * Implementation of the Move native function `fun fresh_id(): address`
 **************************************************************************************************/
pub fn fresh_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_fresh_id_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_fresh_id_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_fresh_id_cost_params.tx_context_fresh_id_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let fresh_id = transaction_context.fresh_id();
    let object_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;
    object_runtime.new_id(fresh_id)?;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(fresh_id.into())],
    ))
}

#[derive(Clone)]
pub struct TxContextSenderCostParams {
    pub tx_context_sender_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_sender
 * Implementation of the Move native function `fun native_sender(): address`
 **************************************************************************************************/
pub fn sender(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_sender_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_sender_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_sender_cost_params.tx_context_sender_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let sender = transaction_context.sender();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(sender.into())],
    ))
}

#[derive(Clone)]
pub struct TxContextEpochCostParams {
    pub tx_context_epoch_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_epoch
 * Implementation of the Move native function `fun native_epoch(): u64`
 **************************************************************************************************/
pub fn epoch(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_epoch_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_epoch_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_epoch_cost_params.tx_context_epoch_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let epoch = transaction_context.epoch();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(epoch)],
    ))
}

#[derive(Clone)]
pub struct TxContextEpochTimestampMsCostParams {
    pub tx_context_epoch_timestamp_ms_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_epoch_timestamp_ms
 * Implementation of the Move native function `fun native_epoch_timestamp_ms(): u64`
 **************************************************************************************************/
pub fn epoch_timestamp_ms(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_epoch_timestamp_ms_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_epoch_timestamp_ms_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_epoch_timestamp_ms_cost_params.tx_context_epoch_timestamp_ms_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let timestamp = transaction_context.epoch_timestamp_ms();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(timestamp)],
    ))
}

#[derive(Clone)]
pub struct TxContextSponsorCostParams {
    pub tx_context_sponsor_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_sponsor
 * Implementation of the Move native function `fun native_sponsor(): Option<address>`
 **************************************************************************************************/
pub fn sponsor(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_sponsor_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_sponsor_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_sponsor_cost_params.tx_context_sponsor_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let sponsor = transaction_context
        .sponsor()
        .map(|addr| addr.into())
        .into_iter();
    let sponsor = Value::vector_address(sponsor);
    Ok(NativeResult::ok(context.gas_used(), smallvec![sponsor]))
}

#[derive(Clone)]
pub struct TxContextGasPriceCostParams {
    pub tx_context_gas_price_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_gas_price
 * Implementation of the Move native function `fun native_gas_price(): u64`
 **************************************************************************************************/
pub fn gas_price(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_gas_price_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_gas_price_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_gas_price_cost_params.tx_context_gas_price_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let gas_price = transaction_context.gas_price();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(gas_price)],
    ))
}

#[derive(Clone)]
pub struct TxContextGasBudgetCostParams {
    pub tx_context_gas_budget_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_gas_budget
 * Implementation of the Move native function `fun native_gas_budget(): u64`
 **************************************************************************************************/
pub fn gas_budget(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_gas_budget_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_gas_budget_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_gas_budget_cost_params.tx_context_gas_budget_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let gas_budget = transaction_context.gas_budget();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(gas_budget)],
    ))
}

#[derive(Clone)]
pub struct TxContextIdsCreatedCostParams {
    pub tx_context_ids_created_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_ids_created
 * Implementation of the Move native function `fun native_ids_created(): u64`
 **************************************************************************************************/
pub fn ids_created(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_ids_created_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_ids_created_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_ids_created_cost_params.tx_context_ids_created_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let ids_created = transaction_context.ids_created();

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::u64(ids_created)],
    ))
}

// //
// // Test only function
// //
#[derive(Clone)]
pub struct TxContextReplaceCostParams {
    pub tx_context_replace_cost_base: InternalGas,
}
/***************************************************************************************************
 * native fun native_replace
 * Implementation of the Move native function
 * ```
 * fun native_replace(
 *   sender: address,
 *   tx_hash: vector<u8>,
 *   epoch: u64,
 *   epoch_timestamp_ms: u64,
 *   ids_created: u64,
 * )
 * ```
 * Used by all testing functions that have to change a value in the `TransactionContext`.
 **************************************************************************************************/
pub fn replace(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    mut args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.len() == 8);

    let tx_context_replace_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_replace_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_replace_cost_params.tx_context_replace_cost_base
    );

    let mut sponsor: Vec<AccountAddress> = pop_arg!(args, Vec<AccountAddress>);
    let gas_budget: u64 = pop_arg!(args, u64);
    let gas_price: u64 = pop_arg!(args, u64);
    let ids_created: u64 = pop_arg!(args, u64);
    let epoch_timestamp_ms: u64 = pop_arg!(args, u64);
    let epoch: u64 = pop_arg!(args, u64);
    let tx_hash: Vec<u8> = pop_arg!(args, Vec<u8>);
    let sender: AccountAddress = pop_arg!(args, AccountAddress);
    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    transaction_context.replace(
        sender,
        tx_hash,
        epoch,
        epoch_timestamp_ms,
        ids_created,
        gas_price,
        gas_budget,
        sponsor.pop(),
    )?;

    Ok(NativeResult::ok(context.gas_used(), smallvec![]))
}
// Attempt to get the most recent created object ID when none has been created.
// Lifted out of Move into this native function.
const E_NO_IDS_CREATED: u64 = 1;

// use same protocol config and cost value as derive_id
/***************************************************************************************************
 * native fun last_created_id
 * Implementation of the Move native function `fun last_created_id(): address`
 **************************************************************************************************/
pub fn last_created_id(
    context: &mut NativeContext,
    ty_args: Vec<Type>,
    args: VecDeque<Value>,
) -> PartialVMResult<NativeResult> {
    debug_assert!(ty_args.is_empty());
    debug_assert!(args.is_empty());

    let tx_context_derive_id_cost_params = context
        .extensions_mut()
        .get::<NativesCostTable>()?
        .tx_context_derive_id_cost_params
        .clone();
    native_charge_gas_early_exit!(
        context,
        tx_context_derive_id_cost_params.tx_context_derive_id_cost_base
    );

    let transaction_context: &mut TransactionContext = context.extensions_mut().get_mut()?;
    let mut ids_created = transaction_context.ids_created();
    if ids_created == 0 {
        return Ok(NativeResult::err(context.gas_used(), E_NO_IDS_CREATED));
    }
    ids_created -= 1;
    let digest = transaction_context.digest();
    let address = AccountAddress::from(ObjectID::derive_id(digest, ids_created));
    let obj_runtime: &mut ObjectRuntime = context.extensions_mut().get_mut()?;
    obj_runtime.new_id(address.into())?;

    Ok(NativeResult::ok(
        context.gas_used(),
        smallvec![Value::address(address)],
    ))
}
