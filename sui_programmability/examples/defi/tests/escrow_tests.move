// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module defi::escrow_tests {
    use sui::id::{Self, VersionedID};
    use sui::test_scenario::{Self, Scenario};
    use sui::tx_context::{Self};

    use defi::escrow::{Self, EscrowedObj};

    const ALICE_ADDRESS: address = @0xACE;
    const BOB_ADDRESS: address = @0xACEB;
    const THIRD_PARTY_ADDRESS: address = @0xFACE;
    const RANDOM_ADDRESS: address = @123;

    // Error codes.
    const ESwapTransferFailed: u64 = 0;
    const EReturnTransferFailed: u64 = 0;

    // Example of an object type used for exchange
    struct ItemA has key, store {
        id: VersionedID
    }

    // Example of the other object type used for exchange
    struct ItemB has key, store {
        id: VersionedID
    }

    #[test]
    fun test_escrow_flow() {
        // Both Alice and Bob send items to the third party
        let scenario = &mut send_to_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        swap(scenario, &THIRD_PARTY_ADDRESS);

        // Alice now owns item B, and Bob now owns item A
        assert!(owns_object<ItemB>(scenario, &ALICE_ADDRESS), ESwapTransferFailed);
        assert!(owns_object<ItemA>(scenario, &BOB_ADDRESS), ESwapTransferFailed);
    }

    #[test]
    fun test_return_to_sender() {
        // Both Alice and Bob send items to the third party
        let scenario = &mut send_to_escrow(ALICE_ADDRESS, BOB_ADDRESS);

        // The third party returns item A to Alice, item B to Bob
        test_scenario::next_tx(scenario, &THIRD_PARTY_ADDRESS);
        {
            let item_a = test_scenario::take_owned<EscrowedObj<ItemA, ItemB>>(scenario);
            escrow::return_to_sender<ItemA, ItemB>(item_a);

            let item_b = test_scenario::take_owned<EscrowedObj<ItemB, ItemA>>(scenario);
            escrow::return_to_sender<ItemB, ItemA>(item_b);
        };

        // Alice now owns item A, and Bob now owns item B
        assert!(owns_object<ItemA>(scenario, &ALICE_ADDRESS), EReturnTransferFailed);
        assert!(owns_object<ItemB>(scenario, &BOB_ADDRESS), EReturnTransferFailed);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_swap_wrong_objects() {
        // Both Alice and Bob send items to the third party except that Alice wants to exchange
        // for a different object than Bob's
        let scenario = &mut send_to_escrow_with_overrides(ALICE_ADDRESS, BOB_ADDRESS, true, false);
        swap(scenario, &THIRD_PARTY_ADDRESS);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_swap_wrong_recipient() {
        // Both Alice and Bob send items to the third party except that Alice put a different
        // recipient than Bob
        let scenario = &mut send_to_escrow_with_overrides(ALICE_ADDRESS, BOB_ADDRESS, false, true);
        swap(scenario, &THIRD_PARTY_ADDRESS);
    }

    fun swap(scenario: &mut Scenario, third_party: &address) {
        test_scenario::next_tx(scenario, third_party);
        {
            let item_a = test_scenario::take_owned<EscrowedObj<ItemA, ItemB>>(scenario);
            let item_b = test_scenario::take_owned<EscrowedObj<ItemB, ItemA>>(scenario);
            escrow::swap(item_a, item_b);
        };
    }

    fun send_to_escrow(
        alice: address,
        bob: address,
    ): Scenario {
        send_to_escrow_with_overrides(alice, bob, false, false)
    }

    fun send_to_escrow_with_overrides(
        alice: address,
        bob: address,
        override_exchange_for: bool,
        override_recipient: bool,
    ): Scenario {
        let new_scenario = test_scenario::begin(&alice);
        let scenario = &mut new_scenario;
        let ctx = test_scenario::ctx(scenario);
        let item_a_versioned_id = tx_context::new_id(ctx);

        test_scenario::next_tx(scenario, &bob);
        let ctx = test_scenario::ctx(scenario);
        let item_b_versioned_id = tx_context::new_id(ctx);

        let item_a_id = *id::inner(&item_a_versioned_id);
        let item_b_id = *id::inner(&item_b_versioned_id);
        if (override_exchange_for) {
            item_b_id = id::new(RANDOM_ADDRESS);
        };

        // Alice sends item A to the third party
        test_scenario::next_tx(scenario, &alice);
        {
            let ctx = test_scenario::ctx(scenario);
            let escrowed = ItemA {
                id: item_a_versioned_id
            };
            let recipient = bob;
            if (override_recipient) {
                recipient = RANDOM_ADDRESS;
            };
            escrow::create<ItemA, ItemB>(
                recipient,
                THIRD_PARTY_ADDRESS,
                item_b_id,
                escrowed,
                ctx
            );
        };

        // Bob sends item B to the third party
        test_scenario::next_tx(scenario, &BOB_ADDRESS);
        {
            let ctx = test_scenario::ctx(scenario);
            let escrowed = ItemB {
                id: item_b_versioned_id
            };
            escrow::create<ItemB, ItemA>(
                alice,
                THIRD_PARTY_ADDRESS,
                item_a_id,
                escrowed,
                ctx
            );
        };
        new_scenario
    }

    fun owns_object<T: key + store>(scenario: &mut Scenario, owner: &address): bool{
        test_scenario::next_tx(scenario, owner);
        test_scenario::can_take_owned<T>(scenario)
    }
}
