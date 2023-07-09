// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module kiosk::owned_kiosk_rule {
    use sui::kiosk::{Self, Kiosk};
    use sui::transfer_policy::{
        Self as policy,
        TransferPolicy,
        TransferPolicyCap,
        TransferRequest
    };

    use kiosk::owned_kiosk;

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
    public fun confirm<T>(kiosk: &Kiosk, request: &mut TransferRequest<T>) {
        assert!(kiosk::has_item(kiosk, policy::item(request)), EItemNotInKiosk);
        assert!(owned_kiosk::is_owned(kiosk), EKioskNotOwned);

        policy::add_receipt(Rule {}, request)
    }

}
