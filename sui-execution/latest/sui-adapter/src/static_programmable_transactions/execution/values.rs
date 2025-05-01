// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use std::sync::atomic::AtomicU64;

use crate::static_programmable_transactions::{env::Env, typing::ast::Type};
use move_binary_format::errors::PartialVMError;
use move_core_types::account_address::AccountAddress;
use move_trace_format::value;
use move_vm_types::values::{self, Struct, VMValueCast, Value};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    error::ExecutionError,
    object::Owner,
};
pub enum InputValue {
    Bytes(ByteValue),
    Fixed(Option<Value>),
    Object(InputObjectValue),
}

pub enum ByteValue {
    Pure(Vec<u8>),
    Receiving {
        id: ObjectID,
        version: SequenceNumber,
    },
}

#[derive(Clone, Debug)]
pub struct InputObjectMetadata {
    pub id: ObjectID,
    pub is_mutable_input: bool,
    pub owner: Owner,
    pub version: SequenceNumber,
}

pub struct InputObjectValue {
    pub object_metadata: InputObjectMetadata,
    pub value: Option<Value>,
}

pub fn load_value(_env: &Env, _bytes: &[u8], _ty: Type) -> Result<Value, ExecutionError> {
    todo!("RUNTIME")
}

pub fn borrow_value(_value: &Value) -> Result<Value, ExecutionError> {
    todo!("RUNTIME")
}

pub fn copy_value(value: &Value) -> Result<Value, ExecutionError> {
    value.copy_value().map_err(iv("copy"))
}

pub fn read_ref(value: Value) -> Result<Value, ExecutionError> {
    let value: values::Reference = value.cast().map_err(iv("cast"))?;
    value.read_ref().map_err(iv("read ref"))
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

pub fn uid(address: AccountAddress) -> Value {
    Value::struct_(Struct::pack([Value::address(address)]))
}

pub fn receiving(id: ObjectID, version: SequenceNumber) -> Value {
    Value::struct_(Struct::pack([uid(id.into()), Value::u64(version.into())]))
}

//**************************************************************************************************
// Coin Functions
//**************************************************************************************************

pub fn coin_value(coin_ref: Value) -> Result<u64, ExecutionError> {
    let balance_value_ref = borrow_coin_balance_value(coin_ref)?;
    let balance_value_ref: values::Reference = balance_value_ref.cast().map_err(iv("cast"))?;
    let balance_value = balance_value_ref.read_ref().map_err(iv("read ref"))?;
    balance_value.cast().map_err(iv("cast"))
}

/// The coin value MUST be checked before calling this function, if `amount` is greater than
/// the value of the coin, it will return an invariant violation.
pub fn coin_subtract_balance(coin_ref: Value, amount: u64) -> Result<(), ExecutionError> {
    coin_modify_balance(coin_ref, |balance| {
        let Some(new_balance) = balance.checked_sub(amount) else {
            invariant_violation!("coin balance {balance} is less than {amount}")
        };
        Ok(new_balance)
    })
}

/// The coin max value MUST be checked before calling this function, if `amount` plus the current
/// balance is greater than `u64::MAX`, it will return an invariant violation.
pub fn coin_add_balance(coin_ref: Value, amount: u64) -> Result<(), ExecutionError> {
    coin_modify_balance(coin_ref, |balance| {
        let Some(new_balance) = balance.checked_add(amount) else {
            invariant_violation!("coin balance {balance} + {amount} is greater than u64::MAX")
        };
        Ok(new_balance)
    })
}

fn coin_modify_balance(
    coin_ref: Value,
    modify: impl FnOnce(u64) -> Result<u64, ExecutionError>,
) -> Result<(), ExecutionError> {
    let balance_value_ref = borrow_coin_balance_value(coin_ref)?;
    let reference: values::Reference = balance_value_ref
        .copy_value()
        .map_err(iv("copy"))?
        .cast()
        .map_err(iv("cast"))?;
    let balance: u64 = reference
        .read_ref()
        .map_err(iv("read ref"))?
        .cast()
        .map_err(iv("cast"))?;
    let new_balance = modify(balance)?;
    let reference: values::Reference = balance_value_ref.cast().map_err(iv("cast"))?;
    reference
        .write_ref(Value::u64(new_balance))
        .map_err(iv("write ref"))
}

fn borrow_coin_balance_value(coin_ref: Value) -> Result<Value, ExecutionError> {
    let coin_ref: values::StructRef = coin_ref.cast().map_err(iv("cast"))?;
    let balance = coin_ref.borrow_field(1).map_err(iv("borrow field"))?;
    let balance: values::StructRef = balance.cast().map_err(iv("cast"))?;
    balance.borrow_field(0).map_err(iv("borrow field"))
}

const fn iv(case: &str) -> impl FnOnce(PartialVMError) -> ExecutionError + use<'_> {
    move |e| make_invariant_violation!("unexpected {case} failure {e:?}")
}
