// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

#[test_only]
module DeFi::SharedEscrowTests {
    use Sui::ID::{Self, VersionedID};
    use Sui::TestScenario::{Self, Scenario};
    use Sui::TxContext::{Self};

    use DeFi::SharedEscrow::{Self, EscrowedObj};

    const ALICE_ADDRESS: address = @0xACE;
    const BOB_ADDRESS: address = @0xACEB;
    const THIRD_PARTY_ADDRESS: address = @0xFACE;
    const RANDOM_ADDRESS: address = @123;

    // Error codes.
    const ESWAP_TRANSFER_FAILED: u64 = 0;
    const ERETURN_TRANSFER_FAILED: u64 = 0;

    // Example of an object type used for exchange
    struct ItemA has key, store {
        id: VersionedID
    }

    // Example of the other object type used for exchange
    struct ItemB has key, store {
        id: VersionedID
    }

    #[test]
    public(script) fun test_escrow_flow() {
        // Alice creates the escrow
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);

        // Bob exchanges item B for the escrowed item A
        exchange(&mut scenario, &BOB_ADDRESS, item_b_versioned_id);

        // Alice now owns item B, and Bob now owns item A
        assert!(owns_object<ItemB>(&mut scenario, &ALICE_ADDRESS), ESWAP_TRANSFER_FAILED);
        assert!(owns_object<ItemA>(&mut scenario, &BOB_ADDRESS), ESWAP_TRANSFER_FAILED);
    }

    #[test]
    public(script) fun test_cancel() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        ID::delete(id);
        let scenario = &mut scenario;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(scenario, &ALICE_ADDRESS), ERETURN_TRANSFER_FAILED);

        // Alice cancels the escrow
        cancel(scenario, &ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(scenario, &ALICE_ADDRESS), ERETURN_TRANSFER_FAILED);
    }

    #[test]
    #[expected_failure(abort_code = 0)]
    public(script) fun test_cancel_with_wrong_owner() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        ID::delete(id);
        let scenario = &mut scenario;

        // Bob tries to cancel the escrow that Alice owns and expects failure
        cancel(scenario, &BOB_ADDRESS);
    }

    #[test]
    #[expected_failure(abort_code = 2)]
    public(script) fun test_swap_wrong_objects() {
        // Alice creates the escrow in exchange for item b
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        ID::delete(item_b_versioned_id);
        let scenario = &mut scenario;

        // Bob tries to exchange item C for the escrowed item A and expects failure
        TestScenario::next_tx(scenario, &BOB_ADDRESS);
        let ctx = TestScenario::ctx(scenario);
        let item_c_versioned_id = TxContext::new_id(ctx);
        exchange(scenario, &BOB_ADDRESS, item_c_versioned_id);
    }

    #[test]
    #[expected_failure(abort_code = 1)]
    public(script) fun test_swap_wrong_recipient() {
         // Alice creates the escrow in exchange for item b
        let (scenario, item_b_versioned_id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        let scenario = &mut scenario;

        // Random address tries to exchange item B for the escrowed item A and expects failure
        exchange(scenario, &RANDOM_ADDRESS, item_b_versioned_id);
    }

    #[test]
    #[expected_failure(abort_code = 3)]
    public(script) fun test_cancel_twice() {
        // Alice creates the escrow
        let (scenario, id) = create_escrow(ALICE_ADDRESS, BOB_ADDRESS);
        ID::delete(id);
        let scenario = &mut scenario;
        // Alice does not own item A
        assert!(!owns_object<ItemA>(scenario, &ALICE_ADDRESS), ERETURN_TRANSFER_FAILED);

        // Alice cancels the escrow
        cancel(scenario, &ALICE_ADDRESS);

        // Alice now owns item A
        assert!(owns_object<ItemA>(scenario, &ALICE_ADDRESS), ERETURN_TRANSFER_FAILED);

        // Alice tries to cancel the escrow again
        cancel(scenario, &ALICE_ADDRESS);
    }

    public(script) fun cancel(scenario: &mut Scenario, initiator: &address) {
        TestScenario::next_tx(scenario, initiator);
        {
            let escrow_wrapper = TestScenario::take_shared_object<EscrowedObj<ItemA, ItemB>>(scenario);
            let escrow = TestScenario::borrow_mut(&mut escrow_wrapper);
            let ctx = TestScenario::ctx(scenario);
            SharedEscrow::cancel(escrow, ctx);
            TestScenario::return_shared_object(scenario, escrow_wrapper);
        };
    }

    public(script) fun exchange(scenario: &mut Scenario, bob: &address, item_b_verioned_id: VersionedID) {
        TestScenario::next_tx(scenario, bob);
        {
            let escrow_wrapper = TestScenario::take_shared_object<EscrowedObj<ItemA, ItemB>>(scenario);
            let escrow = TestScenario::borrow_mut(&mut escrow_wrapper);
            let item_b = ItemB {
                id: item_b_verioned_id
            };
            let ctx = TestScenario::ctx(scenario);
            SharedEscrow::exchange(item_b, escrow, ctx);
            TestScenario::return_shared_object(scenario, escrow_wrapper);
        };
    }

    fun create_escrow(
        alice: address,
        bob: address,
    ): (Scenario, VersionedID) {
        let new_scenario = TestScenario::begin(&alice);
        let scenario = &mut new_scenario;
        let ctx = TestScenario::ctx(scenario);
        let item_a_versioned_id = TxContext::new_id(ctx);

        TestScenario::next_tx(scenario, &bob);
        let ctx = TestScenario::ctx(scenario);
        let item_b_versioned_id = TxContext::new_id(ctx);
        let item_b_id = *ID::inner(&item_b_versioned_id);

        // Alice creates the escrow
        TestScenario::next_tx(scenario, &alice);
        {
            let ctx = TestScenario::ctx(scenario);
            let escrowed = ItemA {
                id: item_a_versioned_id
            };
            SharedEscrow::create<ItemA, ItemB>(
                bob,
                item_b_id,
                escrowed,
                ctx
            );
        };
        (new_scenario, item_b_versioned_id)
    }

    fun owns_object<T: key + store>(scenario: &mut Scenario, owner: &address): bool{
        TestScenario::next_tx(scenario, owner);
        TestScenario::can_take_object<T>(scenario)
    }
}
