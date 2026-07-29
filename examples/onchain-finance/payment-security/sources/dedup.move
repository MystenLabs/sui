// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::dedup;

use sui::event;
use sui::table::{Self, Table};

#[error]
const EDuplicatePayment: vector<u8> = b"Payment ID already processed";

public struct PaymentProcessed has copy, drop {
    payment_id: vector<u8>,
}

// docs::#dedup
public struct PaymentRegistry has key {
    id: UID,
    processed: Table<vector<u8>, bool>,
}

fun init(ctx: &mut TxContext) {
    let registry = PaymentRegistry {
        id: object::new(ctx),
        processed: table::new(ctx),
    };
    transfer::share_object(registry);
}

/// Call this from your payment/settlement function to prevent duplicates.
/// Aborts if the payment_id was already processed.
public fun process_payment(registry: &mut PaymentRegistry, payment_id: vector<u8>) {
    assert!(!registry.processed.contains(payment_id), EDuplicatePayment);
    registry.processed.add(payment_id, true);
    event::emit(PaymentProcessed { payment_id });
}
// docs::/#dedup
