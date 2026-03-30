// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Base Types
///
/// Foundational types used throughout the dependency chain.
module leaf::base {
    /// A unique identifier.
    public struct Id has copy, drop, store {
        inner: u64,
    }

    /// Create a new Id.
    public fun new_id(v: u64): Id {
        Id { inner: v }
    }

    /// Get the inner value.
    public fun inner(id: &Id): u64 {
        id.inner
    }
}
