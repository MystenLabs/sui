// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// An example module implementing a fixed commission for the `TransferPolicy`.
/// Follows the "transfer rules" layout and implements each of the steps.
module sui::fixed_commission {
    use sui::sui::SUI;
    use sui::coin::Coin;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferRequest,
        TransferPolicyCap
    };

    /// Expected amount does not match the passed one.
    const EIncorrectAmount: u64 = 0;

    /// Custom witness-key which also acts as a key for the policy.
    struct Rule has drop {}

    /// Fixed commission on all sales.
    struct Commission has store, drop { amount: u64 }

    /// Creator action: adds a Rule;
    /// Set a FixedCommission requirement for the TransferPolicy.
    public fun set<T>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>,
        amount: u64
    ) {
        policy::add_rule(Rule {}, policy, cap, Commission { amount });
    }

    /// Creator action: remove the rule from the policy.
    /// Can be performed freely at any time, this method only helps fill-in type params.
    public fun unset<T>(policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>) {
        policy::remove_rule<T, Rule, Commission>(policy, cap)
    }

    /// Buyer action: perform required action;
    /// Complete the requirement on `TransferRequest`. In this case - pay the fixed fee.
    public fun pay<T>(
        policy: &mut TransferPolicy<T>, request: &mut TransferRequest<T>, coin: Coin<SUI>
    ) {
        let paid = policy::paid(request);
        let config: &Commission = policy::get_rule(Rule {}, policy);

        assert!(paid == config.amount, EIncorrectAmount);

        policy::add_to_balance(Rule {}, policy, coin);
        policy::add_receipt(Rule {}, request);
    }
}
