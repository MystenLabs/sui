// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module defines a Rule which forces buyers to put the purchased
/// item into the Kiosk and lock it. The most common use case for the
/// Rule is making sure an item never leaves Kiosks and has policies
/// enforced on every transfer.
///
/// Configuration:
/// - None
///
/// Use cases:
/// - Enforcing policies on every trade
/// - Making sure an item never leaves the Kiosk / certain ecosystem
///
/// Notes:
/// - "locking" mechanic disallows the `kiosk::take` function and forces
/// the owner to use `list` or `list_with_purchase_cap` methods if they
/// wish to move the item somewhere else.
///
module kiosk::kiosk_lock_rule {
    use sui::kiosk::{Self, Kiosk};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    /// Item is not in the `Kiosk`.
    const ENotInKiosk: u64 = 0;

    /// The type identifier for the Rule.
    struct Rule has drop {}

    /// An empty configuration for the Rule.
    struct Config has store, drop {}

    /// Creator: Adds a `kiosk_lock_rule` Rule to the `TransferPolicy` forcing
    /// buyers to lock the item in a Kiosk on purchase.
    public fun add<T>(policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>) {
        policy::add_rule(Rule {}, policy, cap, Config {})
    }

    /// Buyer: Prove the item was locked in the Kiosk to get the receipt and
    /// unblock the transfer request confirmation.
    public fun prove<T>(request: &mut TransferRequest<T>, kiosk: &Kiosk) {
        let item = policy::item(request);
        assert!(kiosk::has_item(kiosk, item) && kiosk::is_locked(kiosk, item), ENotInKiosk);
        policy::add_receipt(Rule {}, request)
    }
}
