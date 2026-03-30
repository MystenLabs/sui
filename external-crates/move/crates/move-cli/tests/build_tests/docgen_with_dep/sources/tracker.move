// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Tracker
///
/// Uses `helper::utils::Counter` from a dependency package.
///
/// Nested list in module doc:
///
/// - Top level
///   - Nested item
///     - Deeply nested
/// - Another top level
module app::tracker {
    use helper::utils::{Self, Counter};

    /// A named tracker wrapping a counter.
    public struct Tracker has copy, drop {
        name: u64,
        counter: Counter,
    }

    /// Create a new tracker.
    public fun new(name: u64): Tracker {
        Tracker {
            name,
            counter: utils::new(),
        }
    }

    /// Record an event.
    public fun record(t: &mut Tracker) {
        utils::increment(&mut t.counter);
    }

    /// Get event count.
    public fun count(t: &Tracker): u64 {
        utils::value(&t.counter)
    }
}
