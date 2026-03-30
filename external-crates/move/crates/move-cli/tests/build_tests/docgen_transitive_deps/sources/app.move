// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # App
///
/// Root application using types from both direct (Mid) and transitive (Leaf) dependencies.
module root::app {
    use mid::registry::{Self, Entry};
    use leaf::base;

    /// Create an entry and return its inner id value.
    public fun create_and_read(v: u64, tag: u64): u64 {
        let entry: Entry = registry::new_entry(v, tag);
        let id = registry::entry_id(&entry);
        base::inner(id)
    }
}
