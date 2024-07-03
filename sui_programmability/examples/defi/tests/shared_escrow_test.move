// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module defi::shared_escrow_tests {
    use sui::test_scenario::{Self, Scenario};

    use defi::shared_escrow::{Self, EscrowedObj};

    const ALICE_ADDRESS: address = @0xACE;
    const BOB_ADDRESS: address = @0xACEB;
    const RANDOM_ADDRESS: address = @123;

    // Error codes.
    const ESwapTransferFailed: u64 = 0;
    const EReturnTransferFailed: u64 = 0;

    // Example of an object type used for exchange
    public struct ItemA has key, store {
        id: UID
    }

    // Example of the other object type used for exchange
    public struct ItemB has key, store {
        id: UID
    }

    #[test]
    fun test_escrow_flow() {
        // Alice creates the escrow
        let (mut scenario_val, item_b) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);

        // Bob exchanges item B for the escrowed item A
        exchange(&mut scenario_val, BOB_ADDRESS, item_b);

        // Alice now owns item B, and Bob now owns item A
        assert!(owns_object<ItemB>(ALICE_ADDRESS), ESwapTransferFailed);
        assert!(owns_object<ItemA>(BOB_ADDRESS), ESwapTransferFailed);

        scenario_val.end();
    }

    #[test]
    fun test_cancel() {
        // Alice creates the escrow
        let (mut scenario_val, ItemB { id }) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        id.delete();
        let scenario = &mut scenario_val;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(ALICE_ADDRESS), EReturnTransferFailed);

        // Alice cancels the escrow
        cancel(scenario, ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(ALICE_ADDRESS), EReturnTransferFailed);

        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = shared_escrow::EWrongOwner)]
    fun test_cancel_with_wrong_owner() {
        // Alice creates the escrow
        let (mut scenario_val, ItemB { id }) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        id.delete();
        let scenario = &mut scenario_val;

        // Bob tries to cancel the escrow that Alice owns and expects failure
        cancel(scenario, BOB_ADDRESS);

        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = shared_escrow::EWrongExchangeObject)]
    fun test_swap_wrong_objects() {
        // Alice creates the escrow in exchange for item b
        let (mut scenario_val, ItemB { id }) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        id.delete();
        let scenario = &mut scenario_val;

        // Bob tries to exchange item C for the escrowed item A and expects failure
        scenario.next_tx(BOB_ADDRESS);
        let ctx = scenario.ctx();
        let item_c = ItemB { id: object::new(ctx) };
        exchange(scenario, BOB_ADDRESS, item_c);

        scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = shared_escrow::EWrongRecipient)]
    fun test_swap_wrong_recipient() {
         // Alice creates the escrow in exchange for item b
        let (mut scenario_val, item_b) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        let scenario = &mut scenario_val;

        // Random address tries to exchange item B for the escrowed item A and expects failure
        exchange(scenario, RANDOM_ADDRESS, item_b);

         scenario_val.end();
    }

    #[test]
    #[expected_failure(abort_code = shared_escrow::EAlreadyExchangedOrCancelled)]
    fun test_cancel_twice() {
        // Alice creates the escrow
        let (mut scenario_val, ItemB { id }) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        id.delete();
        let scenario = &mut scenario_val;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(ALICE_ADDRESS), EReturnTransferFailed);

        // Alice cancels the escrow
        cancel(scenario, ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(ALICE_ADDRESS), EReturnTransferFailed);

        // Alice tries to cancel the escrow again
        cancel(scenario, ALICE_ADDRESS);

        scenario_val.end();
    }

    fun cancel(scenario: &mut Scenario, initiator: address) {
        scenario.next_tx(initiator);
        {
            let mut escrow_val = scenario.take_shared<EscrowedObj<ItemA, ItemB>>();
            let escrow = &mut escrow_val;
            let ctx = scenario.ctx();
            shared_escrow::cancel(escrow, ctx);
            test_scenario::return_shared(escrow_val);
        };
        scenario.next_tx(initiator);
    }

    fun exchange(scenario: &mut Scenario, bob: address, item_b: ItemB) {
        scenario.next_tx(bob);
        {
            let mut escrow_val = scenario.take_shared<EscrowedObj<ItemA, ItemB>>();
            let escrow = &mut escrow_val;
            let ctx = scenario.ctx();
            shared_escrow::exchange(item_b, escrow, ctx);
            test_scenario::return_shared(escrow_val);
        };
        scenario.next_tx(bob);
    }

    fun create_escrow(
        alice: address,
        bob: address,
    ): (Scenario, ItemB) {
        let mut new_scenario = test_scenario::begin(alice);
        let scenario = &mut new_scenario;
        let ctx = scenario.ctx();
        let item_a_versioned_id = object::new(ctx);

        scenario.next_tx(bob);
        let ctx = scenario.ctx();
        let item_b = ItemB { id: object::new(ctx) };
        let item_b_id = object::id(&item_b);

        // Alice creates the escrow
        scenario.next_tx(alice);
        {
            let ctx = scenario.ctx();
            let escrowed = ItemA {
                id: item_a_versioned_id
            };
            shared_escrow::create<ItemA, ItemB>(
                bob,
                item_b_id,
                escrowed,
                ctx
            );
        };
        scenario.next_tx(alice);
        (new_scenario, item_b)
    }

    fun owns_object<T: key + store>(owner: address): bool {
        test_scenario::has_most_recent_for_address<T>(owner)
    }
}
