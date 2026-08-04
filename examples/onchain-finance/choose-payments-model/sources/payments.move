// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::payments;

use sui::balance::{Self, Balance};
use sui::event;

#[error(code = 0)]
const EInsufficientPayment: vector<u8> = b"Payment amount is less than required";
#[error(code = 1)]
const ENotAuthorized: vector<u8> = b"Caller is not authorized to withdraw";
#[error(code = 2)]
const ENothingCollected: vector<u8> = b"No funds are available to withdraw";

public struct PaymentReceived<phantom T> has copy, drop {
    config_id: ID,
    payer: address,
    amount: u64,
}

// docs::#payments
/// A typed merchant checkout that takes custody of accepted payments.
public struct PaymentConfig<phantom T> has key {
    id: UID,
    collector: address,
    min_amount: u64,
    collected: Balance<T>,
}

public fun create_config<T>(collector: address, min_amount: u64, ctx: &mut TxContext) {
    transfer::share_object(PaymentConfig<T> {
        id: object::new(ctx),
        collector,
        min_amount,
        collected: balance::zero(),
    });
}

/// Accept a payment supplied from an address balance or converted coin.
public fun pay<T>(config: &mut PaymentConfig<T>, payment: Balance<T>, ctx: &TxContext) {
    let amount = payment.value();
    assert!(amount >= config.min_amount, EInsufficientPayment);
    config.collected.join(payment);
    event::emit(PaymentReceived<T> {
        config_id: object::id(config),
        payer: ctx.sender(),
        amount,
    });
}

/// Send all accepted funds to the configured collector's address balance.
public fun withdraw<T>(config: &mut PaymentConfig<T>, ctx: &TxContext) {
    assert!(ctx.sender() == config.collector, ENotAuthorized);
    assert!(config.collected.value() > 0, ENothingCollected);
    balance::send_funds(config.collected.withdraw_all(), config.collector);
}
// docs::/#payments
