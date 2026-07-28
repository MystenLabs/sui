// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::payments;

use sui::balance::{Self, Balance};
use sui::coin::{Self, Coin};
use sui::event;

#[error]
const EInsufficientPayment: vector<u8> = b"Payment amount is less than required";
#[error]
const ENotAuthorized: vector<u8> = b"Caller is not authorized to withdraw";

public struct PaymentReceived has copy, drop {
    payer: address,
    amount: u64,
}

// docs::#payments
/// A shared payment configuration that accepts deposits of a specific coin type.
/// Only the authorized collector can withdraw accumulated funds.
public struct PaymentConfig<phantom T> has key {
    id: UID,
    collector: address,
    min_amount: u64,
    collected: Balance<T>,
}

public fun create_config<T>(collector: address, min_amount: u64, ctx: &mut TxContext) {
    let config = PaymentConfig<T> {
        id: object::new(ctx),
        collector,
        min_amount,
        collected: balance::zero(),
    };
    transfer::share_object(config);
}

/// Pay into the config. The module controls the funds after deposit.
public fun pay<T>(config: &mut PaymentConfig<T>, payment: Coin<T>, ctx: &TxContext) {
    assert!(payment.value() >= config.min_amount, EInsufficientPayment);
    let amount = payment.value();
    config.collected.join(payment.into_balance());
    event::emit(PaymentReceived { payer: ctx.sender(), amount });
}

/// Withdraw collected funds. Only the designated collector can call this.
public fun withdraw<T>(config: &mut PaymentConfig<T>, ctx: &mut TxContext): Coin<T> {
    assert!(ctx.sender() == config.collector, ENotAuthorized);
    let amount = config.collected.value();
    coin::from_balance(config.collected.split(amount), ctx)
}
// docs::/#payments
