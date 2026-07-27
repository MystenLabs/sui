// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// docs::#snippet_1
use sui::clock::Clock;

const EPaymentExpired: u64 = 0;

public fun pay_with_deadline(
    clock: &Clock,
    deadline_ms: u64,
) {
    assert!(clock.timestamp_ms() <= deadline_ms, EPaymentExpired);
    // ... process payment
}
// docs::/snippet_1
