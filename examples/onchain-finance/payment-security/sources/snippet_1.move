// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module example::payment_deadline;

use sui::clock::Clock;

#[error]
const EPaymentExpired: vector<u8> = b"Payment deadline has passed";

// docs::#payment-deadline
public fun pay_with_deadline(clock: &Clock, deadline_ms: u64) {
    assert!(clock.timestamp_ms() <= deadline_ms, EPaymentExpired);
    // ... process payment
}
// docs::/#payment-deadline
