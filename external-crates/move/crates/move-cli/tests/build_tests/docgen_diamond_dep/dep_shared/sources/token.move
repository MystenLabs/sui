// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Token
///
/// A shared token type used by multiple packages.
module shared::token {
    /// A token with a value.
    public struct Token has copy, drop, store {
        value: u64,
    }

    /// Create a new token.
    public fun mint(value: u64): Token {
        Token { value }
    }

    /// Get the token value.
    public fun value(t: &Token): u64 {
        t.value
    }
}
