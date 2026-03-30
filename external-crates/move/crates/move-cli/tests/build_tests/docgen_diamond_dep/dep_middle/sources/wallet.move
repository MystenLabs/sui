// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Wallet
///
/// A wallet holding a `shared::token::Token`.
module middle::wallet {
    use shared::token::{Self, Token};

    /// A wallet with a single token.
    public struct Wallet has copy, drop {
        held: Token,
    }

    /// Create a wallet with a freshly minted token.
    public fun new(amount: u64): Wallet {
        Wallet { held: token::mint(amount) }
    }

    /// Get the balance.
    public fun balance(w: &Wallet): u64 {
        token::value(&w.held)
    }
}
