// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a simple deny_rule which uses a VecSet to store blocked
/// addresses. In future, we will provide a more efficient implementation of
/// deny_rule which uses a better data structure.
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

    /// User is in the list.
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

    /// Verifies that the request does not exceed the limit and adds an approval
    /// to the `ActionRequest`.
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
    fun denylist_pass_not_on_the_list() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        // create an empty denylist and then populate it with records
        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x1 ], ctx);

        let request = cl::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        // try to confirm request with 100 tokens
        cl::confirm_request(
            &mut policy,
            request,
            ctx
        );

        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    fun denylist_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        // create an empty denylist and then populate it with records
        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x0 ], ctx);

        let request = cl::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    fun denylist_recipient_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (policy, cap) = test::get_policy(ctx);

        // create an empty denylist and then populate it with records
        denylist::add_for(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records_for(&mut policy, &cap, utf8(b"action"), vector[ @0x1 ], ctx);

        let request = cl::new_request(utf8(b"action"), 100, some(@0x1), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }
}
