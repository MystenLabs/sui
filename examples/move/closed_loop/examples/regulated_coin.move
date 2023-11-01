// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::regulated_coin {
    use std::option;
    use sui::transfer;
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{sender, TxContext};

    use closed_loop::closed_loop as cl;

    // import rules and use them for this app
    use examples::allowlist_rule as allowlist;
    use examples::denylist_rule as denylist;
    use examples::limiter_rule as limiter;

    /// OTW and the type for the Token.
    struct REGULATED_COIN has drop {}

    // Most of the magic happens in the initializer for the demonstration
    // purposes; however half of what's happening here could be implemented as
    // a single / set of PTBs.
    fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
        let treasury_cap = create_currency(otw, ctx);
        let (policy, cap) = cl::new(&treasury_cap, ctx);

        // Create a denylist rule and add it to every action
        // Now all actions are allowed but require a denylist
        denylist::add_for(&mut policy, &cap, cl::spend_action(), ctx);
        denylist::add_for(&mut policy, &cap, cl::to_coin_action(), ctx);
        denylist::add_for(&mut policy, &cap, cl::transfer_action(), ctx);
        denylist::add_for(&mut policy, &cap, cl::from_coin_action(), ctx);

        // Set limits for each action:
        // transfer - 3000.00 REG, to_coin - 1000.00 REG
        limiter::add_for(&mut policy, &cap, cl::transfer_action(), 3000_000000, ctx);
        limiter::add_for(&mut policy, &cap, cl::to_coin_action(), 1000_000000, ctx);

        // Using allowlist to mock a KYC process; transfer and from_coin can
        // only be performed by KYC-d (allowed) addresses. Just like a Bank
        // account.
        allowlist::add_for(&mut policy, &cap, cl::from_coin_action(), ctx);
        allowlist::add_for(&mut policy, &cap, cl::transfer_action(), ctx);

        transfer::public_transfer(treasury_cap, sender(ctx));
        transfer::public_transfer(cap, sender(ctx));
        cl::share_policy(policy);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init`.
    public(friend) fun create_currency<T: drop>(otw: T, ctx: &mut TxContext): TreasuryCap<T> {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6,
            b"REG",
            b"Regulated Coin",
            b"Coin that illustrates different regulatory requirements",
            option::none(),
            ctx
        );

        transfer::public_freeze_object(metadata);
        treasury_cap
    }
}
