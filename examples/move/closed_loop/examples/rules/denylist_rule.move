// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An implementation of a simple `DenyList` for the Closed Loop system. For
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
    use sui::tx_context::TxContext;
    use sui::vec_set::{Self, VecSet};
    use closed_loop::closed_loop::{
        Self as cl,
        TokenPolicy,
        TokenPolicyCap,
        ActionRequest
    };

    /// Trying to `verify` but the sender or the recipient is on the denylist.
    const EUserBlocked: u64 = 0;

    /// The Rule witness.
    struct DenyList has drop {}

    /// Adds a limiter rule to the `TokenPolicy` with the given limit per
    /// operation.
    public fun add_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        name: String,
        ctx: &mut TxContext
    ) {
        cl::add_rule_for_action(
            DenyList {}, policy, cap, name, vec_set::empty<address>(), ctx
        );
    }

    /// Verifies that the sender and the recipient (if set) are not on the
    /// denylist for the given action.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext
    ) {
        let sender = cl::sender(request);
        let receiver = cl::recipient(request);
        let denylist: &VecSet<address> = cl::get_rule(
            DenyList {}, policy, cl::name(request)
        );

        assert!(!vec_set::contains(denylist, &sender), EUserBlocked);

        if (option::is_some(&receiver)) {
            assert!(!vec_set::contains(
                denylist, option::borrow(&receiver)
            ), EUserBlocked);
        };

        cl::add_approval(DenyList {}, request, ctx);
    }

    /// Removes the `denylist_rule` for a given action.
    public fun remove_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        ctx: &mut TxContext
    ) {
        let _ = cl::remove_rule_for_action<T, DenyList, VecSet<address>>(
            policy, cap, action, ctx
        );
    }

    // === Protected: List Management ===

    /// Adds records to the `denylist_rule` for a given action. The Policy
    /// owner can batch-add records.
    public fun add_records_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        addresses: vector<address>,
        ctx: &mut TxContext
    ) {
        let denylist = cl::get_rule_for_action_mut<T, DenyList, VecSet<address>>(
            policy, cap, action, ctx
        );

        while (vector::length(&addresses) > 0) {
            let new_record = vector::pop_back(&mut addresses);
            if (!vec_set::contains(denylist, &new_record)) {
                vec_set::insert(denylist, new_record);
            };
        };
    }

    /// Removes records from the `denylist_rule` for a given action. The Policy
    /// owner can batch-remove records.
    public fun remove_records_for<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        action: String,
        addresses: vector<address>,
        ctx: &mut TxContext
    ) {
        let denylist = cl::get_rule_for_action_mut<T, DenyList, VecSet<address>>(
            policy, cap, action, ctx
        );

        while (vector::length(&addresses) > 0) {
            let record = vector::pop_back(&mut addresses);
            if (vec_set::contains(denylist, &record)) {
                vec_set::remove(denylist, &record);
            };
        };
    }
}

#[test_only]
module examples::denylist_rule_tests {
    use examples::denylist_rule as denylist;
    use std::string::utf8;
    use std::option::{none, some};
    use closed_loop::closed_loop as cl;
    use closed_loop::closed_loop_tests as test;

    #[test]
    // Scenario: add a denylist with addresses, sender is not on the list and
    // transaction is confirmed.
    fun denylist_pass_not_on_the_list() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        // first add the list for action and then add records
        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x1 ], ctx);

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
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x0 ], ctx);

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
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x1 ], ctx);

        let request = cl::new_request(utf8(b"action"), 100, some(@0x1), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }
}
