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
    use sui::{bag::{Self, Bag}, token::{Self, TokenPolicy, TokenPolicyCap, ActionRequest}};

    /// Trying to `verify` but the sender or the recipient is on the denylist.
    const EUserBlocked: u64 = 0;

    /// The Rule witness.
    public struct Denylist has drop {}

    /// Verifies that the sender and the recipient (if set) are not on the
    /// denylist for the given action.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext,
    ) {
        // early return if no records are added;
        if (!has_config(policy)) {
            token::add_approval(Denylist {}, request, ctx);
            return
        };

        let config = config(policy);
        let sender = token::sender(request);
        let receiver = token::recipient(request);

        assert!(!bag::contains(config, sender), EUserBlocked);

        if (option::is_some(&receiver)) {
            let receiver = *option::borrow(&receiver);
            assert!(!bag::contains(config, receiver), EUserBlocked);
        };

        token::add_approval(Denylist {}, request, ctx);
    }

    // === Protected: List Management ===

    /// Adds records to the `denylist_rule` for a given action. The Policy
    /// owner can batch-add records.
    public fun add_records<T>(
        policy: &mut TokenPolicy<T>,
        cap: &TokenPolicyCap<T>,
        mut addresses: vector<address>,
        ctx: &mut TxContext,
    ) {
        if (!has_config(policy)) {
            token::add_rule_config(Denylist {}, policy, cap, bag::new(ctx), ctx);
        };

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
        mut addresses: vector<address>,
        _ctx: &mut TxContext,
    ) {
        let config_mut = config_mut(policy, cap);

        while (vector::length(&addresses) > 0) {
            let record = vector::pop_back(&mut addresses);
            if (bag::contains(config_mut, record)) {
                let _: bool = bag::remove(config_mut, record);
            };
        };
    }

    // === Internal ===

    fun has_config<T>(self: &TokenPolicy<T>): bool {
        token::has_rule_config_with_type<T, Denylist, Bag>(self)
    }

    fun config<T>(self: &TokenPolicy<T>): &Bag {
        token::rule_config<T, Denylist, Bag>(Denylist {}, self)
    }

    fun config_mut<T>(self: &mut TokenPolicy<T>, cap: &TokenPolicyCap<T>): &mut Bag {
        token::rule_config_mut(Denylist {}, self, cap)
    }
}

#[test_only]
module examples::denylist_rule_tests {
    use examples::denylist_rule::{Self as denylist, Denylist};
    use std::{option::{none, some}, string::utf8};
    use sui::{token, token_test_utils::{Self as test, TEST}};

    #[test]
    // Scenario: add a denylist with addresses, sender is not on the list and
    // transaction is confirmed.
    fun denylist_pass_not_on_the_list() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        // first add the list for action and then add records
        token::add_rule_for_action<TEST, Denylist>(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[@0x1], ctx);

        let mut request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);
        token::confirm_request(&policy, request, ctx);
        test::return_policy(policy, cap);
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    // Scenario: add a denylist with addresses, sender is on the list and
    // transaction fails with `EUserBlocked`.
    fun denylist_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        token::add_rule_for_action<TEST, Denylist>(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[@0x0], ctx);

        let mut request = token::new_request(utf8(b"action"), 100, none(), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }

    #[test, expected_failure(abort_code = examples::denylist_rule::EUserBlocked)]
    // Scenario: add a denylist with addresses, Recipient is on the list and
    // transaction fails with `EUserBlocked`.
    fun denylist_recipient_on_the_list_banned_fail() {
        let ctx = &mut sui::tx_context::dummy();
        let (mut policy, cap) = test::get_policy(ctx);

        token::add_rule_for_action<TEST, Denylist>(&mut policy, &cap, utf8(b"action"), ctx);
        denylist::add_records(&mut policy, &cap, vector[@0x1], ctx);

        let mut request = token::new_request(utf8(b"action"), 100, some(@0x1), none(), ctx);

        denylist::verify(&policy, &mut request, ctx);

        abort 1337
    }
}
