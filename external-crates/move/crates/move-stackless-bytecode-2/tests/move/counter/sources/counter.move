// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module counter::counter {

    public struct Counter has drop {
        value: u64,
    }

    public fun value(counter: &Counter): u64 {
        counter.value
    }

    /// Create and share a Counter object.
    public fun create(): Counter {
        Counter {
            value: 0
        }
    }

    /// Increment a counter by 1.
    public fun increment(counter: &mut Counter) {
        counter.value = counter.value + 1;
    }

    /// Set value (only runnable by the Counter owner)
    public fun set_value(counter: &mut Counter, value: u64) {
        counter.value = value;
    }
}
