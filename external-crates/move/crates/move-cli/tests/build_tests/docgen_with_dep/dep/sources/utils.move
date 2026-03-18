// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Utilities
///
/// Shared utility types.
module helper::utils {
    /// A simple counter.
    public struct Counter has copy, drop, store {
        value: u64,
    }

    /// Create a new counter starting at zero.
    public fun new(): Counter {
        Counter { value: 0 }
    }

    /// Increment by one.
    public fun increment(c: &mut Counter) {
        c.value = c.value + 1;
    }

    /// Get the current value.
    public fun value(c: &Counter): u64 {
        c.value
    }
}
