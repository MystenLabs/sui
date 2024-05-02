// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Test illustrating how an asset can be forever locked in the Kiosk.
module sui::kiosk_locked_test {
    use sui::item_locked_policy as locked_policy;
    use sui::kiosk_test_utils::{Self as test, Asset};

    #[test]
    fun test_item_always_locked() {
        let ctx = &mut test::ctx();
        let (_, _, carl) = test::folks();
        let (mut policy, policy_cap) = test::get_policy(ctx);
        let (mut kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (item, item_id) = test::get_asset(ctx);
        let payment = test::get_sui(1000, ctx);

        // Alice the Creator
        // - disallow taking from the Kiosk
        // - require "PlacedWitness" on purchase
        // - place an asset into the Kiosk so it can only be sold
        locked_policy::set(&mut policy, &policy_cap);
        kiosk.lock(&kiosk_cap, &policy, item);
        kiosk.list<Asset>(&kiosk_cap, item_id, 1000);

        // Bob the Buyer
        // - places the item into his Kiosk and gets the proof
        // - prove placing and confirm request
        let (mut bob_kiosk, bob_kiosk_cap) = test::get_kiosk(ctx);
        let (item, mut request) = kiosk.purchase<Asset>(item_id, payment);
        bob_kiosk.lock(&bob_kiosk_cap, &policy, item);

        // The difference!
        locked_policy::prove(&mut request, &bob_kiosk);
        policy.confirm_request(request);

        // Carl the Cleaner;
        // - cleans up and transfer Kiosk to himself
        // - as we can't take an item due to the policy setting (and kiosk must be empty)
        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, kiosk_cap, ctx);

        transfer::public_transfer(bob_kiosk, carl);
        transfer::public_transfer(bob_kiosk_cap, carl);
    }
}
