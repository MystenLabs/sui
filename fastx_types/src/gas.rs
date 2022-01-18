// Copyright (c) Facebook, Inc. and its affiliates.
// SPDX-License-Identifier: Apache-2.0

use crate::{
    error::{FastPayError, FastPayResult},
    gas_coin::GasCoin,
    messages::{Order, OrderKind},
    object::Object,
};
use std::convert::TryFrom;

macro_rules! ok_or_gas_error {
    ($cond:expr, $e:expr) => {
        if !($cond) {
            Err(FastPayError::InsufficientGas { error: $e })
        } else {
            Ok(())
        }
    };
}

const MIN_MOVE_CALL_GAS: u64 = 10;
const MIN_MOVE_PUBLISH_GAS: u64 = 10;
const MIN_OBJ_TRANSFER_GAS: u64 = 8;

pub fn check_gas_requirement(order: &Order, gas_object: &Object) -> FastPayResult {
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

/// Subtract the gas balance of \p gas_object by \p amount.
/// \p amount being positive means gas deduction; \p amount being negative
/// means gas refund.
pub fn deduct_gas(gas_object: &mut Object, amount: i128) -> FastPayResult {
    let gas_coin = GasCoin::try_from(&*gas_object)?;
    let balance = gas_coin.value() as i128;
    let new_balance = balance - amount;
    ok_or_gas_error!(
        new_balance >= 0,
        format!("Gas balance is {}, not enough to pay {}", balance, amount)
    )?;
    ok_or_gas_error!(
        new_balance <= u64::MAX as i128,
        format!(
            "Gas balance is {}, overflow after reclaiming {}",
            balance, -amount
        )
    )?;
    let new_gas_coin = GasCoin::new(*gas_coin.id(), gas_object.version(), new_balance as u64);
    let move_object = gas_object.data.try_as_move_mut().unwrap();
    move_object.update_contents(bcs::to_bytes(&new_gas_coin).unwrap());
    Ok(())
}

pub fn get_gas_balance(gas_object: &Object) -> FastPayResult<u64> {
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
