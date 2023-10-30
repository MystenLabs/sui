// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Implements a simple CLI Closed Loop Token which is used for demonstration
/// purposes.
module cli::cli {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    use closed_loop::closed_loop as cl;

    /// OTW and the Type for the CL Token.
    struct CLI has drop {}

    fun init(otw: CLI, ctx: &mut TxContext) {
        let (treasury_cap, coin_metadata) = coin::create_currency(
            otw,
            0, // no decimals
            b"CLI", // symbol
            b"CLI Token", // name
            b"Token to test CLI application", // description
            option::none(), // url
            ctx
        );

        let (policy, policy_cap) = cl::new(&mut treasury_cap, ctx);

        cl::share_policy(policy);
        
        transfer::public_transfer(policy_cap, tx_context::sender(ctx));
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_freeze_object(coin_metadata);
    }
}
