// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::settlement;

use std::ascii::String;
use std::type_name;
use sui::balance::{Self, Balance};
use sui::event;

#[error(code = 0)]
const EPaymentTooSmall: vector<u8> = b"Payment amount is less than the required amount";

public struct PaymentSettled<phantom T> has copy, drop {
    payer: address,
    recipient: address,
    amount: u64,
    required_amount: u64,
    coin_type: String,
    reference: vector<u8>,
}

// docs::#settlement
/// Atomically validates, transfers, and records a payment for backend reconciliation.
public fun settle<T>(
    payment: Balance<T>,
    recipient: address,
    required_amount: u64,
    reference: vector<u8>,
    ctx: &TxContext,
) {
    let amount = payment.value();
    assert!(amount >= required_amount, EPaymentTooSmall);

    // `into_string` already yields an `ascii::String`; no further conversion is needed.
    let coin_type = type_name::with_defining_ids<T>().into_string();
    balance::send_funds(payment, recipient);
    event::emit(PaymentSettled<T> {
        payer: ctx.sender(),
        recipient,
        amount,
        required_amount,
        coin_type,
        reference,
    });
}
// docs::/#settlement
