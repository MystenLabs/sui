// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{SuiError, SuiResult},
    gas_coin::GasCoin,
    messages::{Order, OrderKind},
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

pub const MIN_MOVE_CALL_GAS: u64 = 10;
pub const MIN_MOVE_PUBLISH_GAS: u64 = 10;
pub const MIN_OBJ_TRANSFER_GAS: u64 = 8;

pub fn check_gas_requirement(order: &Order, gas_object: &Object) -> SuiResult {
    match &order.kind {
        OrderKind::Transfer(t) => {
            debug_assert_eq!(t.gas_payment.0, gas_object.id());
            let balance = get_gas_balance(gas_object)?;
            ok_or_gas_error!(
                balance >= MIN_OBJ_TRANSFER_GAS,
                format!(
                    "Gas balance is {}, smaller than minimum requirement of {} for object transfer.",
                    balance, MIN_OBJ_TRANSFER_GAS
                )
            )
        }
        OrderKind::Publish(publish) => {
            debug_assert_eq!(publish.gas_payment.0, gas_object.id());
            let balance = get_gas_balance(gas_object)?;
            ok_or_gas_error!(
                balance >= MIN_MOVE_PUBLISH_GAS,
                format!(
                    "Gas balance is {}, smaller than minimum requirement of {} for module publish.",
                    balance, MIN_MOVE_PUBLISH_GAS
                )
            )
        }
        OrderKind::Call(call) => {
            debug_assert_eq!(call.gas_payment.0, gas_object.id());
            ok_or_gas_error!(
                call.gas_budget >= MIN_MOVE_CALL_GAS,
                format!(
                    "Gas budget is {}, smaller than minimum requirement of {} for move call.",
                    call.gas_budget, MIN_MOVE_CALL_GAS
                )
            )?;
            let balance = get_gas_balance(gas_object)?;
            ok_or_gas_error!(
                balance >= call.gas_budget,
                format!(
                    "Gas balance is {}, smaller than the budget {} for move call.",
                    balance, call.gas_budget
                )
            )
        }
    }
}

/// Try subtract the gas balance of \p gas_object by \p amount.
pub fn try_deduct_gas(gas_object: &mut Object, amount: u64) -> SuiResult {
    // The object must be a gas coin as we have checked in order handle phase.
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
    module_bytes.iter().map(|v| v.len() as u64).sum::<u64>() + MIN_MOVE_PUBLISH_GAS
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
