// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use super::base_types::*;
use crate::{
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    messages::{Transaction, TransactionKind},
    object::{Object, Owner},
};
use std::convert::TryFrom;

macro_rules! ok_or_gas_error {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            Err(SuiError::InsufficientGas { error: $e })
        } else {
            Ok(())
        }
    };
}

pub const MIN_MOVE: u64 = 10;
pub const MIN_OBJ_TRANSFER_GAS: u64 = 8;

pub fn check_gas_requirement(transaction: &Transaction, gas_object: &Object) -> SuiResult {
    debug_assert_eq!(transaction.gas_payment_object_ref().0, gas_object.id());
    ok_or_gas_error!(
        matches!(gas_object.owner, Owner::AddressOwner(..)),
        "Gas object must be owned by the signer".to_string()
    )?;
    match &transaction.data.kind {
        TransactionKind::Transfer(_) => {
            let balance = get_gas_balance(gas_object)?;
            ok_or_gas_error!(
                balance >= MIN_OBJ_TRANSFER_GAS,
                format!(
                    "Gas balance is {}, smaller than minimum requirement of {} for object transfer.",
                    balance, MIN_OBJ_TRANSFER_GAS
                )
            )
        }
        TransactionKind::Call(op) => check_move_gas_requirement(
            gas_object,
            transaction.gas_payment_object_ref(),
            op.gas_budget,
        ),
        TransactionKind::Publish(op) => check_move_gas_requirement(
            gas_object,
            transaction.gas_payment_object_ref(),
            op.gas_budget,
        ),
    }
}

pub fn check_move_gas_requirement(
    gas_object: &Object,
    gas_payment: &ObjectRef,
    gas_budget: u64,
) -> SuiResult {
    debug_assert_eq!(gas_payment.0, gas_object.id());
    ok_or_gas_error!(
        gas_budget >= MIN_MOVE,
        format!(
            "Gas budget is {}, smaller than minimum requirement of {} for move operation.",
            gas_budget, MIN_MOVE
        )
    )?;
    let balance = get_gas_balance(gas_object)?;
    ok_or_gas_error!(
        balance >= gas_budget,
        format!(
            "Gas balance is {}, smaller than the budget {} for move operation.",
            balance, gas_budget
        )
    )
}

/// Try subtract the gas balance of \p gas_object by \p amount.
pub fn try_deduct_gas(gas_object: &mut Object, amount: u64) -> SuiResult {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    ok_or_gas_error!(
        balance >= amount,
        format!("Gas balance is {}, not enough to pay {}", balance, amount)
    )?;
    let new_gas_coin = GasCoin::new(*gas_coin.id(), gas_object.version(), balance - amount);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents(bcs::to_bytes(&new_gas_coin).unwrap());
    Ok(())
}

pub fn check_gas_balance(gas_object: &Object, amount: u64) -> SuiResult {
    let balance = get_gas_balance(gas_object)?;
    ok_or_gas_error!(
        balance >= amount,
        format!("Gas balance is {}, not enough to pay {}", balance, amount)
    )
}

/// Subtract the gas balance of \p gas_object by \p amount.
/// This function should not fail, and should only be called in cases when
/// we know for sure there is enough balance.
pub fn deduct_gas(gas_object: &mut Object, amount: u64) {
    try_deduct_gas(gas_object, amount).unwrap();
}

pub fn get_gas_balance(gas_object: &Object) -> SuiResult<u64> {
    Ok(GasCoin::try_from(gas_object)?.value())
}

pub fn calculate_module_publish_cost(module_bytes: &[Vec<u8>]) -> u64 {
    // TODO: Figure out module publish gas formula.
    // Currently just use the size in bytes of the modules plus a default minimum.
    module_bytes.iter().map(|v| v.len() as u64).sum::<u64>() + MIN_MOVE
}

pub fn calculate_object_transfer_cost(object: &Object) -> u64 {
    // TODO: Figure out object transfer gas formula.
    (object.data.try_as_move().unwrap().contents().len() / 2) as u64
}

pub fn calculate_object_creation_cost(object: &Object) -> u64 {
    // TODO: Figure out object creation gas formula.
    object.data.try_as_move().unwrap().contents().len() as u64
}

pub fn calculate_object_deletion_refund(object: &Object) -> u64 {
    // TODO: Figure out object creation gas formula.
    (object.data.try_as_move().unwrap().contents().len() / 2) as u64
}

pub fn aggregate_gas(gas_used: u64, gas_refund: u64) -> u64 {
    // Cap gas refund by half of gas_used.
    let gas_refund = std::cmp::min(gas_used / 2, gas_refund);
    gas_used - gas_refund
}
