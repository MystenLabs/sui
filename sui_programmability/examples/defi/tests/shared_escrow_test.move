// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module defi::shared_escrow_tests {
    use sui::object::{Self, Info};
    use sui::test_scenario::{Self, Scenario};

    use defi::shared_escrow::{Self, EscrowedObj};

    const ALICE_ADDRESS: address = @0xACE;
    const BOB_ADDRESS: address = @0xACEB;
    const THIRD_PARTY_ADDRESS: address = @0xFACE;
    const RANDOM_ADDRESS: address = @123;

    // Error codes.
    const ESwapTransferFailed: u64 = 0;
    const EReturnTransferFailed: u64 = 0;

    // Example of an object type used for exchange
    struct ItemA has key, store {
        info: Info
    }

    // Example of the other object type used for exchange
    struct ItemB has key, store {
        info: Info
    }

    #[test]
    fun test_escrow_flow() {
        // Alice creates the escrow
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);

        // Bob exchanges item B for the escrowed item A
        exchange(&mut scenario, &BOB_ADDRESS, item_b_versioned_id);

        // Alice now owns item B, and Bob now owns item A
        assert!(owns_object<ItemB>(&mut scenario, &ALICE_ADDRESS), ESwapTransferFailed);
        assert!(owns_object<ItemA>(&mut scenario, &BOB_ADDRESS), ESwapTransferFailed);
    }

    #[test]
    fun test_cancel() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        object::delete(id);
        let scenario = &mut scenario;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(scenario, &ALICE_ADDRESS), EReturnTransferFailed);

        // Alice cancels the escrow
        cancel(scenario, &ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(scenario, &ALICE_ADDRESS), EReturnTransferFailed);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    fun test_cancel_with_wrong_owner() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        object::delete(id);
        let scenario = &mut scenario;

        // Bob tries to cancel the escrow that Alice owns and expects failure
        cancel(scenario, &BOB_ADDRESS);
    }

    #[test]
    #[expected_failure(abort_code = 2)]
    fun test_swap_wrong_objects() {
        // Alice creates the escrow in exchange for item b
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        object::delete(item_b_versioned_id);
        let scenario = &mut scenario;

        // Bob tries to exchange item C for the escrowed item A and expects failure
        test_scenario::next_tx(scenario, &BOB_ADDRESS);
        let ctx = test_scenario::ctx(scenario);
        let item_c_versioned_id = object::new(ctx);
        exchange(scenario, &BOB_ADDRESS, item_c_versioned_id);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    fun test_swap_wrong_recipient() {
         // Alice creates the escrow in exchange for item b
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        let scenario = &mut scenario;

        // Random address tries to exchange item B for the escrowed item A and expects failure
        exchange(scenario, &RANDOM_ADDRESS, item_b_versioned_id);
    }

    #[test]
    #[expected_failure(abort_code = 3)]
    fun test_cancel_twice() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        object::delete(id);
        let scenario = &mut scenario;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(scenario, &ALICE_ADDRESS), EReturnTransferFailed);

        // Alice cancels the escrow
        cancel(scenario, &ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(scenario, &ALICE_ADDRESS), EReturnTransferFailed);

        // Alice tries to cancel the escrow again
        cancel(scenario, &ALICE_ADDRESS);
    }

    fun cancel(scenario: &mut Scenario, initiator: &address) {
        test_scenario::next_tx(scenario, initiator);
        {
            let escrow_wrapper = test_scenario::take_shared<EscrowedObj<ItemA, ItemB>>(scenario);
            let escrow = test_scenario::borrow_mut(&mut escrow_wrapper);
            let ctx = test_scenario::ctx(scenario);
            shared_escrow::cancel(escrow, ctx);
            test_scenario::return_shared(scenario, escrow_wrapper);
        };
    }

    fun exchange(scenario: &mut Scenario, bob: &address, item_b_verioned_id: Info) {
        test_scenario::next_tx(scenario, bob);
        {
            let escrow_wrapper = test_scenario::take_shared<EscrowedObj<ItemA, ItemB>>(scenario);
            let escrow = test_scenario::borrow_mut(&mut escrow_wrapper);
            let item_b = ItemB {
                info: item_b_verioned_id
            };
            let ctx = test_scenario::ctx(scenario);
            shared_escrow::exchange(item_b, escrow, ctx);
            test_scenario::return_shared(scenario, escrow_wrapper);
        };
    }

    fun create_escrow(
        alice: address,
        bob: address,
    ): (Scenario, Info) {
        let new_scenario = test_scenario::begin(&alice);
        let scenario = &mut new_scenario;
        let ctx = test_scenario::ctx(scenario);
        let item_a_versioned_id = object::new(ctx);

        test_scenario::next_tx(scenario, &bob);
        let ctx = test_scenario::ctx(scenario);
        let item_b_versioned_id = object::new(ctx);
        let item_b_id = *object::info_id(&item_b_versioned_id);

        // Alice creates the escrow
        test_scenario::next_tx(scenario, &alice);
        {
            let ctx = test_scenario::ctx(scenario);
            let escrowed = ItemA {
                info: item_a_versioned_id
            };
            shared_escrow::create<ItemA, ItemB>(
                bob,
                item_b_id,
                escrowed,
                ctx
            );
        };
        (new_scenario, item_b_versioned_id)
    }

    fun owns_object<T: key + store>(scenario: &mut Scenario, owner: &address): bool{
        test_scenario::next_tx(scenario, owner);
        test_scenario::can_take_owned<T>(scenario)
    }
}
