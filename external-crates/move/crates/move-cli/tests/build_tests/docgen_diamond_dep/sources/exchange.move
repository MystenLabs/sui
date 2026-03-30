// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// # Exchange
///
/// Uses `shared::token` directly AND `middle::wallet` which also depends on `shared::token`.
/// This creates a diamond dependency: Diamond → Shared, Diamond → Middle → Shared.
module diamond::exchange {
    use shared::token::{Self, Token};
    use middle::wallet::{Self, Wallet};

    /// Mint a token and put it in a wallet, then read both values.
    public fun round_trip(amount: u64): (u64, u64) {
        let t: Token = token::mint(amount);
        let w: Wallet = wallet::new(amount);
        (token::value(&t), wallet::balance(&w))
    }
}
