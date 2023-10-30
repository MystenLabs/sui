// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An implementation of a simple `Denylist` for the Closed Loop system. For
/// demonstration purposes it is implemented as a `VecSet`, however for a larger
/// number of records there needs to be a different storage implementation
/// utilizing dynamic fields.
///
/// Denylist checks both the sender and the recipient of the transaction.
///
/// Notes:
/// - current implementation uses a separate dataset for each action, which will
/// be fixed / improved in the future;
/// - the current implementation is not optimized for a large number of records
/// and the final one will feature better collection type;
module examples::denylist_rule {
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

    /// Trying to `verify` but the sender or the recipient is on the denylist.
    const EUserBlocked: u64 = 0;

    /// The Rule witness.
    struct Denylist has drop {}

    /// Adds a limiter rule to the `TokenPolicy` with the given limit per
    /// operation.
    public fun add_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        if (!cl::has_rule_config<T, Denylist>(policy)) {
            cl::add_rule_config(Denylist {}, policy, cap, bag::new(ctx), ctx)
        };

        cl::add_rule_for_action(Denylist {}, policy, cap, action, ctx);
    }

    /// Verifies that the sender and the recipient (if set) are not on the
    /// denylist for the given action.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext
    ) {
        let config = config(policy);
        let sender = cl::sender(request);
        let receiver = cl::recipient(request);

        assert!(!bag::contains(config, sender), EUserBlocked);

        if (option::is_some(&receiver)) {
            let receiver = *option::borrow(&receiver);
            assert!(!bag::contains(config, receiver), EUserBlocked);
        };

        cl::add_approval(Denylist {}, request, ctx);
    }

    /// Removes the `denylist_rule` for a given action.
    public fun remove_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        cl::remove_rule_for_action<T, Denylist>(policy, cap, action, ctx);
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
        cl::rule_config<T, Denylist, Bag>(Denylist {}, self)
    }

    fun config_mut<T>(self: &mut TokenPolicy<T>, cap: &TokenPolicyCap<T>): &mut Bag {
        cl::rule_config_mut(Denylist {}, self, cap)
    }
}

#[test_only]
module examples::denylist_rule_tests {
    use examples::denylist_rule as denylist;
    use std::string::utf8;
    use std::option::{none, some};
    use closed_loop::closed_loop as cl;
    use closed_loop::test_utils as test;

    #[test]
    // Scenario: add a denylist with addresses, sender is not on the list and
    // transaction is confirmed.
    fun denylist_pass_not_on_the_list() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        // first add the list for action and then add records
        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[ @0x1 ]);

        let request = cl::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);
        cl::confirm_request(&mut policy, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    // Scenario: add a denylist with addresses, sender is on the list and
    // transaction fails with `EUserBlocked`.
    fun denylist_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[ @0x0 ]);

        let request = cl::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    // Scenario: add a denylist with addresses, Recipient is on the list and
    // transaction fails with `EUserBlocked`.
    fun denylist_recipient_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[ @0x1 ]);

        let request = cl::new_request(utf8(b"action"), 100, some(@0x1), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }
}
