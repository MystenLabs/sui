// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Registry
///
/// A registry that maps `leaf::base::Id` values to names.
module mid::registry {
    use leaf::base::{Self, Id};

    /// A named entry in the registry.
    public struct Entry has copy, drop, store {
        id: Id,
        tag: u64,
    }

    /// Create a new entry.
    public fun new_entry(v: u64, tag: u64): Entry {
        Entry { id: base::new_id(v), tag }
    }

    /// Get the id of an entry.
    public fun entry_id(e: &Entry): &Id {
        &e.id
    }
}
