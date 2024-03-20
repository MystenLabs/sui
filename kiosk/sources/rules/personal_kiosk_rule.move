// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Description:
/// This module defines a Rule which checks that the Kiosk is "personal" meaning
/// that the owner cannot change. By default, `KioskOwnerCap` can be transferred
/// and owned by an application therefore the owner of the Kiosk is not fixed.
///
/// Configuration:
/// - None
///
/// Use cases:
/// - Strong royalty enforcement - personal Kiosks cannot be transferred with
/// the assets inside which means that the item will never change the owner.
///
/// Notes:
/// - Combination of `kiosk_lock_rule` and `personal_kiosk_rule` can be used to
/// enforce policies on every trade (item can be transferred only through a
/// trade + Kiosk is fixed to the owner).
///
module kiosk::personal_kiosk_rule {
    use sui::kiosk::{Self, Kiosk};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    use kiosk::personal_kiosk;

    /// An item hasn't been placed into the Kiosk before the call.
    const EItemNotInKiosk: u64 = 0;
    /// The Kiosk is not owned; the OwnerMarker is not present.
    const EKioskNotOwned: u64 = 1;

    /// The Rule checking that the Kiosk is an owned one.
    struct Rule has drop {}

    /// Add the "owned" rule to the KioskOwnerCap.
    public fun add<T>(policy: &mut TransferPolicy<T>, cap: &TransferPolicyCap<T>) {
        policy::add_rule(Rule {}, policy, cap, true)
    }

    /// Make sure that the destination Kiosk has the Owner key. Item is already
    /// placed by the time this check is performed - otherwise fails.
    public fun prove<T>(kiosk: &Kiosk, request: &mut TransferRequest<T>) {
        assert!(kiosk::has_item(kiosk, policy::item(request)), EItemNotInKiosk);
        assert!(personal_kiosk::is_personal(kiosk), EKioskNotOwned);

        policy::add_receipt(Rule {}, request)
    }
}
