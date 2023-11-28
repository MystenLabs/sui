// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// This example extends on the `simple_token` example but uses immutable policies
/// which are published every once in X epochs. This allows for fast-path
/// execution while still allowing for policy updates; but comes with a burden
/// of supporting and republishing policies.
///
/// Step-by-step:
/// 1. Create a new currency, acquire the TreasuryCap
/// 2. Decide the interval while a single TokenPolicy is active
/// 3. Use the `new_policy_until_epoch` function to create a policy which will
///   be valid until the given epoch. TokenPolicy will be shared.
/// 4. Every X epochs run the `new_policy_until_epoch` function again to create
///  a new policy.
///
/// Benefits:
/// - Fast-path execution - immutable objects are treated as public constants
///
/// Drawbacks:
/// - Requires extra managemenet to support and republish policies
/// - Storage is not reclaimable - old policies will be kept in the storage
///   forever
/// - Denylist needs to be copied every time a new policy is created
/// - Cost of a mistake is high - if a policy is published with a mistake, it
///   cannot be fixed.
module examples::fast_token {
    use std::option;
    use sui::transfer;
    use sui::coin::{Self, TreasuryCap};
    use sui::tx_context::{sender, epoch, TxContext};

    use sui::token::{Self, TokenPolicy, TokenPolicyCap};

    // import rules and use them for this app
    use examples::denylist_rule::{Self, Denylist};
    use examples::before_epoch_rule::{Self, BeforeEpoch};

    /// Trying to set a policy with an epoch in the past will fail.
    const EInvalidEpoch: u64 = 0;

    /// OTW and the type for the Token.
    struct FAST_TOKEN has drop {}

    // Most of the magic happens in the initializer for the demonstration
    // purposes; however half of what's happening here could be implemented as
    // a single / set of PTBs.
    fun init(otw: FAST_TOKEN, ctx: &mut TxContext) {
        let treasury_cap = create_currency(otw, ctx);
        transfer::public_transfer(treasury_cap, sender(ctx));
    }

    /// Create and freeze a `TokenPolicy` with denylist and before-epoch rules.
    public fun new_policy_until_epoch(
        treasury: &mut TreasuryCap<FAST_TOKEN>,
        denylist: vector<address>,
        epoch: u64,
        ctx: &mut TxContext
    ) {
        assert!(epoch(ctx) < epoch, EInvalidEpoch);
        let (policy, cap) = token::new_policy(treasury, ctx);

        // denylist must be copied every time
        denylist_rule::add_records(&mut policy, &cap, denylist, ctx);

        // set the epoch until this policy is valid
        before_epoch_rule::set_epoch(&mut policy, &cap, epoch, ctx);

        set_rules(&mut policy, &cap, ctx);
        token::freeze_policy(policy, cap);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init` and easier testing.
    public(friend) fun set_rules<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        ctx: &mut TxContext
    ) {
        // Create a denylist rule and add it to every action
        // Now all actions are allowed but require a denylist
        token::add_rule_for_action<T, Denylist>(policy, cap, token::spend_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::to_coin_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::transfer_action(), ctx);
        token::add_rule_for_action<T, Denylist>(policy, cap, token::from_coin_action(), ctx);

        // Same with the before-epoch rule
        // Now all actions are allowed but require a denylist and before-epoch verification
        token::add_rule_for_action<T, BeforeEpoch>(policy, cap, token::spend_action(), ctx);
        token::add_rule_for_action<T, BeforeEpoch>(policy, cap, token::to_coin_action(), ctx);
        token::add_rule_for_action<T, BeforeEpoch>(policy, cap, token::transfer_action(), ctx);
        token::add_rule_for_action<T, BeforeEpoch>(policy, cap, token::from_coin_action(), ctx);
    }

    /// Internal: not necessary, but moving this call to a separate function for
    /// better visibility of the Closed Loop setup in `init`.
    fun create_currency<T: drop>(
        otw: T,
        ctx: &mut TxContext
    ): TreasuryCap<T> {
        let (treasury_cap, metadata) = coin::create_currency(
            otw, 6,
            b"FST",
            b"Fast Token",
            b"Token that showcases immutable policy with denylist and epoch rules",
            option::none(),
            ctx
        );

        transfer::public_freeze_object(metadata);
        treasury_cap
    }
}
