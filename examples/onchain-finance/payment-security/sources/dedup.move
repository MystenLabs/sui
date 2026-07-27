// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#dedup
module example::dedup;

use sui::table::{Self, Table};

const EDuplicatePayment: u64 = 0;

public struct PaymentRegistry has key {
    id: UID,
    processed: Table<vector<u8>, bool>,
}

public fun process_payment(
    registry: &mut PaymentRegistry,
    payment_id: vector<u8>,
) {
    assert!(!registry.processed.contains(payment_id), EDuplicatePayment);
    registry.processed.add(payment_id, true);
}
// docs::/dedup
