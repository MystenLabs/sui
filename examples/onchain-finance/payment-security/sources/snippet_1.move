// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::payment_deadline;

use sui::balance::{Self, Balance};
use sui::clock::Clock;

#[error(code = 0)]
const EPaymentExpired: vector<u8> = b"Payment deadline has passed";

// docs::#payment-deadline
/// Check the deadline and complete the payment atomically.
public fun pay_with_deadline<T>(
    payment: Balance<T>,
    recipient: address,
    clock: &Clock,
    deadline_ms: u64,
) {
    assert!(clock.timestamp_ms() <= deadline_ms, EPaymentExpired);
    balance::send_funds(payment, recipient);
}
// docs::/#payment-deadline
