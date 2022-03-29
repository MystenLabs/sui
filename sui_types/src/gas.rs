// Copyright (c) 2021, Facebook, Inc. and its affiliates
// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    object::Object,
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

pub fn check_transfer_gas_requirement(gas_object: &Object, transfer_object: &Object) -> SuiResult {
    let balance = get_gas_balance(gas_object)?;
    let cost = calculate_object_transfer_cost(transfer_object);
    ok_or_gas_error!(
        balance >= cost,
        format!(
            "Gas balance is {}, smaller than gas cost {} for object transfer.",
            balance, cost
        )
    )
}

pub fn check_move_gas_requirement(gas_budget: u64) -> SuiResult {
    ok_or_gas_error!(
        gas_budget >= MIN_MOVE,
        format!(
            "Gas budget is {}, smaller than minimum requirement of {} for move operation.",
            gas_budget, MIN_MOVE
        )
    )?;
    Ok(())
}

/// Try subtract the gas balance of \p gas_object by \p amount.
pub fn try_deduct_gas(gas_object: &mut Object, amount: u64) -> SuiResult {
    // The object must be a gas coin as we have checked in transaction handle phase.
    let gas_coin = GasCoin::try_from(&*gas_object).unwrap();
    let balance = gas_coin.value();
    ok_or_gas_error!(
        balance >= amount,
        format!("Gas balance is {balance}, not enough to pay {amount}")
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
        format!("Gas balance is {balance}, not enough to pay {amount}")
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
