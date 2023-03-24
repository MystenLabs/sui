// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Dummy policy which showcases all of the methods.
module sui::dummy_policy {
    use sui::coin::Coin;
    use sui::sui::SUI;
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    struct Rule has drop {}
    struct Config has store, drop {}

    public fun set<T>(
        policy: &mut TransferPolicy<T>,
        cap: &TransferPolicyCap<T>
    ) {
        policy::add_rule(Rule {}, policy, cap, Config {})
    }

    public fun pay<T>(
        policy: &mut TransferPolicy<T>,
        request: &mut TransferRequest<T>,
        payment: Coin<SUI>
    ) {
        policy::add_to_balance(Rule {}, policy, payment);
        policy::add_receipt(Rule {}, request);
    }
}
