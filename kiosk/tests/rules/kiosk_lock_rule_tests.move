// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Test illustrating how an asset can be forever locked in the Kiosk.
module kiosk::kiosk_lock_rule_tests {
    use sui::kiosk;
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::transfer_policy as policy;
    use sui::transfer;

    use kiosk::kiosk_lock_rule as kiosk_lock;

    #[test]
    fun test_item_always_locked() {
        let ctx = &mut test::ctx();
        let (_, _, carl) = test::folks();
        let (policy, policy_cap) = test::get_policy(ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (item, item_id) = test::get_asset(ctx);
        let payment = test::get_sui(1000, ctx);

        // Alice the Creator
        // - disallow taking from the Kiosk
        // - require "PlacedWitness" on purchase
        // - place an asset into the Kiosk so it can only be sold
        kiosk_lock::add(&mut policy, &policy_cap);
        kiosk::lock(&mut kiosk, &kiosk_cap, &policy, item);
        kiosk::list<Asset>(&mut kiosk, &kiosk_cap, item_id, 1000);

        // Bob the Buyer
        // - places the item into his Kiosk and gets the proof
        // - prove placing and confirm request
        let (bob_kiosk, bob_kiosk_cap) = test::get_kiosk(ctx);
        let (item, request) = kiosk::purchase<Asset>(&mut kiosk, item_id, payment);
        kiosk::lock(&mut bob_kiosk, &bob_kiosk_cap, &policy, item);

        // The difference!
        kiosk_lock::prove(&mut request, &mut bob_kiosk);
        policy::confirm_request(&policy, request);

        // Carl the Cleaner;
        // - cleans up and transfer Kiosk to himself
        // - as we can't take an item due to the policy setting (and kiosk must be empty)
        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, kiosk_cap, ctx);

        transfer::public_transfer(bob_kiosk, carl);
        transfer::public_transfer(bob_kiosk_cap, carl);
    }
}
