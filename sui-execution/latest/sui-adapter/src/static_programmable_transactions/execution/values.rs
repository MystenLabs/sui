// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::static_programmable_transactions::{env::Env, typing::ast::Type};
use move_binary_format::errors::PartialVMError;
use move_core_types::account_address::AccountAddress;
use move_vm_types::values::{self, Struct, VMValueCast, Value};
use sui_types::{
    base_types::{ObjectID, SequenceNumber},
    digests::TransactionDigest,
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
    pub type_: Type,
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

/// This function will invariant violation on an invalid cast
pub fn cast<V>(value: Value) -> Result<V, ExecutionError>
where
    Value: VMValueCast<V>,
{
    value.cast().map_err(iv("cast"))
}

//**************************************************************************************************
// Construction
//**************************************************************************************************

pub fn uid(address: AccountAddress) -> Value {
    // UID { ID { address } }
    Value::struct_(Struct::pack([Value::struct_(Struct::pack([
        Value::address(address),
    ]))]))
}

pub fn receiving(id: ObjectID, version: SequenceNumber) -> Value {
    Value::struct_(Struct::pack([uid(id.into()), Value::u64(version.into())]))
}

pub fn balance(amount: u64) -> Value {
    // Balance { amount }
    Value::struct_(Struct::pack([Value::u64(amount)]))
}

/// The uid _must_ be registered by the object runtime before being called
pub fn coin(id: ObjectID, amount: u64) -> Value {
    Value::struct_(Struct::pack([uid(id.into()), balance(amount)]))
}

pub fn vec_pack(ty: Type, values: Vec<Value>) -> Result<Value, ExecutionError> {
    let ty = {
        ty;
        // ty to vm type
        todo!("LOADING")
    };
    let vec = values::Vector::pack(&ty, values).map_err(iv("pack"))?;
    Ok(Value::struct_(Struct::pack([vec])))
}

pub fn tx_context(digest: TransactionDigest) -> Result<Value, ExecutionError> {
    // public struct TxContext has drop {
    //     sender: address,
    //     tx_hash: vector<u8>,
    //     epoch: u64,
    //     epoch_timestamp_ms: u64,
    //     ids_created: u64,
    // }
    Ok(Value::struct_(Struct::pack([
        Value::address(AccountAddress::ZERO),
        vec_pack(
            Type::U8,
            digest.inner().iter().copied().map(Value::u8).collect(),
        )?,
        Value::u64(0),
        Value::u64(0),
        Value::u64(0),
    ])))
}

//**************************************************************************************************
// Coin Functions
//**************************************************************************************************

pub fn unpack_coin(coin: Value) -> Result<(ObjectID, u64), ExecutionError> {
    let [id, balance] = unpack(coin)?;
    // unpack UID
    let [id] = unpack(id)?;
    // unpack ID
    let [id] = unpack(id)?;
    let id: AccountAddress = id.cast().map_err(iv("cast"))?;
    // unpack Balance
    let [balance] = unpack(balance)?;
    let balance: u64 = balance.cast().map_err(iv("cast"))?;
    Ok((ObjectID::from(id), balance))
}

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

fn unpack<const N: usize>(value: Value) -> Result<[Value; N], ExecutionError> {
    let value: values::Struct = value.cast().map_err(iv("cast"))?;
    let unpacked = value.unpack().map_err(iv("unpack"))?.collect::<Vec<_>>();
    assert_invariant!(unpacked.len() == N, "Expected {N} fields, got {unpacked:?}");
    Ok(unpacked.try_into().unwrap())
}

const fn iv(case: &str) -> impl FnOnce(PartialVMError) -> ExecutionError + use<'_> {
    move |e| make_invariant_violation!("unexpected {case} failure {e:?}")
}
