// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// A simple allowlist rule - allows only the addresses on the allowlist to
/// perform an Action.
module examples::allowlist_rule {
    use std::option;
    use std::vector;
    use std::string::String;
    use sui::bag::{Self, Bag};
    use sui::tx_context::TxContext;
    use closed_loop::closed_loop::{
        Self as cl,
        TokenPolicy,
        TokenPolicyCap,
        ActionRequest
    };

    /// The `sender` or `recipient` is not on the allowlist.
    const ENotAllowed: u64 = 0;

    /// The Rule witness.
    struct Allowlist has drop {}

    /// Adds a `allowlist_rule` for a given action.
    public fun add_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        if (!cl::has_rule_config<T, Allowlist>(policy)) {
            cl::add_rule_config(Allowlist {}, policy, cap, bag::new(ctx), ctx)
        };

        cl::add_rule_for_action(Allowlist {}, policy, cap, action, ctx);
    }

    /// Verifies that the sender and the recipient (if set) are both on the
    /// `allowlist_rule` for a given action.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext
    ) {
        let config = config(policy);
        let sender = cl::sender(request);
        let receiver = cl::recipient(request);

        assert!(bag::contains(config, sender), ENotAllowed);

        if (option::is_some(&receiver)) {
            let receiver = *option::borrow(&receiver);
            assert!(bag::contains(config, receiver), ENotAllowed);
        };

        cl::add_approval(Allowlist {}, request, ctx);
    }

    /// Removes the `denylist_rule` for a given action.
    public fun remove_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        cl::remove_rule_for_action<T, Allowlist>(policy, cap, action, ctx);
    }

    // === Protected: List Management ===

    /// Adds records to the `denylist_rule` for a given action. The Policy
    /// owner can batch-add records.
    public fun add_records<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        addresses: vector<address>,
    ) {
        let config_mut = config_mut(policy, cap);
        while (vector::length(&addresses) > 0) {
            bag::add(config_mut, vector::pop_back(&mut addresses), true)
        }
    }

    /// Removes records from the `denylist_rule` for a given action. The Policy
    /// owner can batch-remove records.
    public fun remove_records<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        addresses: vector<address>,
    ) {
        let config_mut = config_mut(policy, cap);

        while (vector::length(&addresses) > 0) {
            let record = vector::pop_back(&mut addresses);
            let _: bool = bag::remove(config_mut, record);
        };
    }

    // === Internal ===

    fun config<T>(self: &TokenPolicy<T>): &Bag {
        cl::rule_config<T, Allowlist, Bag>(Allowlist {}, self)
    }

    fun config_mut<T>(self: &mut TokenPolicy<T>, cap: &TokenPolicyCap<T>): &mut Bag {
        cl::rule_config_mut(Allowlist {}, self, cap)
    }
}
