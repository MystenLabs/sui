// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::dedup;

use sui::balance::{Self, Balance};
use sui::event;
use sui::table::{Self, Table};

#[error(code = 0)]
const EDuplicatePayment: vector<u8> = b"Payment ID already processed";
#[error(code = 1)]
const EPaymentTooSmall: vector<u8> = b"Payment amount is less than required";

public struct PaymentProcessed<phantom T> has copy, drop {
    registry_id: ID,
    merchant: address,
    payer: address,
    payment_id: vector<u8>,
    amount: u64,
}

// docs::#dedup
/// IDs can only be recorded by atomically depositing funds into this merchant registry.
public struct PaymentRegistry<phantom T> has key {
    id: UID,
    merchant: address,
    processed: Table<vector<u8>, bool>,
    collected: Balance<T>,
}

public fun create_registry<T>(merchant: address, ctx: &mut TxContext) {
    transfer::share_object(PaymentRegistry<T> {
        id: object::new(ctx),
        merchant,
        processed: table::new(ctx),
        collected: balance::zero(),
    });
}

public fun pay<T>(
    registry: &mut PaymentRegistry<T>,
    payment_id: vector<u8>,
    minimum_amount: u64,
    payment: Balance<T>,
    ctx: &TxContext,
) {
    assert!(!registry.processed.contains(payment_id), EDuplicatePayment);
    let amount = payment.value();
    assert!(amount >= minimum_amount, EPaymentTooSmall);
    registry.processed.add(payment_id, true);
    registry.collected.join(payment);
    event::emit(PaymentProcessed<T> {
        registry_id: object::id(registry),
        merchant: registry.merchant,
        payer: ctx.sender(),
        payment_id,
        amount,
    });
}

public fun withdraw<T>(registry: &mut PaymentRegistry<T>) {
    balance::send_funds(registry.collected.withdraw_all(), registry.merchant);
}
// docs::/#dedup
