// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// A Policy that makes sure an item is placed into the `Kiosk` after `purchase`.
/// `Kiosk` can be any.
module sui::item_placed_policy {
    use sui::kiosk::{Self, Kiosk};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// Item is not in the `Kiosk`.
    const ENotInKiosk: u64 = 0;

    /// A unique confirmation for the Rule
    struct Rule has drop {}

    public fun set<T>(policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>) {
        policy::add_rule(Rule {}, policy, cap, true)
    }

    /// Prove that an item a
    public fun prove<T>(request: &mut TransferRequest<T>, kiosk: &Kiosk) {
        assert!(kiosk::has_item(kiosk, policy::item(request)), ENotInKiosk);
        policy::add_receipt(Rule {}, request)
    }
}
