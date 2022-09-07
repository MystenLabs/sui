// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A flash loan that works for any Coin type
module defi::mypool  {
    use sui::transfer;
    use sui::tx_context::{TxContext, sender};
    use sui::sui::SUI;
    use sui::coin::Coin;
    use defi::pool;

    /// The type identifier of coin. The coin will have a type
    /// tag of kind: `Coin<package_object::my_coin::MYCOIN>`
    struct Pool has drop { }

    /// Module initializer is called once on module publish. A treasury
    /// cap is sent to the publisher, who then controls minting and burning
    public entry fun init_pool<T>(
        token: Coin<T>,
        sui: Coin<SUI>,
        fee_percent: u64,
        ctx: &mut TxContext)
    {
        let lp = pool::create_pool(
            Pool { },
            token,
            sui,
            fee_percent,
            ctx
        );
        transfer::transfer(lp, sender(ctx))
    }
}
