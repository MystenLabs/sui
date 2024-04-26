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
module regulated_token::denylist_rule {
    use std::option;
    use std::vector;
    use sui::bag::{Self, Bag};
    use sui::tx_context::TxContext;
    use sui::token::{Self, TokenPolicy, TokenPolicyCap, ActionRequest};

    /// Trying to `verify` but the sender or the recipient is on the denylist.
    const EUserBlocked: u64 = 0;

    /// The Rule witness.
    public struct Denylist has drop {}

    /// Verifies that the sender and the recipient (if set) are not on the
    /// denylist for the given action.
    public fun verify<T>(
        policy: &TokenPolicy<T>,
        request: &mut ActionRequest<T>,
        ctx: &mut TxContext
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
        ctx: &mut TxContext
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
        _ctx: &mut TxContext
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
