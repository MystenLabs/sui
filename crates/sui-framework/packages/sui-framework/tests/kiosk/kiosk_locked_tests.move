// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
/// Test illustrating how an asset can be forever locked in the Kiosk.
module sui::kiosk_locked_test {
    use sui::kiosk::{Self, PlacedWitness};
    use sui::kiosk_test_utils::{Self as test, Asset};
    use sui::transfer_policy as policy;
    use sui::witness_policy;
    use sui::transfer;

    #[test]
    fun test_item_always_locked() {
        let ctx = &mut test::ctx();
        let (_, _, carl) = test::folks();
        let (policy, policy_cap) = test::get_policy(ctx);
        let (kiosk, kiosk_cap) = test::get_kiosk(ctx);
        let (item, item_id) = test::get_asset(ctx);
        let payment = test::get_sui(1000, ctx);

        // Alice the Creator
        // disallow taking from the Kiosk
        // require "PlacedWitness" on purchase
        // place an asset into the Kiosk so it can't be taken
        kiosk::policy_set_no_taking(&mut policy, &policy_cap);
        witness_policy::set<Asset, PlacedWitness<Asset>>(&mut policy, &policy_cap);
        kiosk::place_and_list(&mut kiosk, &kiosk_cap, &policy, item, 1000);

        // Bob the Buyer
        // Places the item into his Kiosk and gets the proof
        // Proves placing and confirmes request
        let (bob_kiosk, bob_kiosk_cap) = test::get_kiosk(ctx);
        let (item, request) = kiosk::purchase<Asset>(&mut kiosk, item_id, payment, ctx);
        let placed_proof = kiosk::place(&mut bob_kiosk, &bob_kiosk_cap, &policy, item);

        witness_policy::prove(placed_proof, &policy, &mut request);
        policy::confirm_request(&policy, request);

        // Carl the Cleaner;
        // Cleans up and transfer Kiosk to himself as we can't take and item
        // from the Bob's Kiosk, and the only option is to transfer it.
        test::return_policy(policy, policy_cap, ctx);
        test::return_kiosk(kiosk, kiosk_cap, ctx);

        transfer::public_transfer(bob_kiosk, carl);
        transfer::public_transfer(bob_kiosk_cap, carl);
    }
}
