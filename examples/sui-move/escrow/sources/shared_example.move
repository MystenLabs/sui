// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects using shared objects without a trusted third party.
/// Swap via shared Escrow 
/// 
/// 1. A user creates a shared object requesting for a specific object ID
/// and posting up their object ID. 
/// 
/// 2. The other user puts in their object which has the same object ID
/// and the exchange is fired in the same txn.
module escrow::shared_example {
    use std::option::{Self, Option};

    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// An object held in escrow
    struct Exchange<T: key + store> has key, store {
        id: UID,
        /// owner of the escrowed object
        sender: address,
        /// intended recipient of the escrowed object
        recipient: address,
        /// ID of the object `sender` wants in exchange
        // TODO: this is probably a bad idea if the object is mutable.
        // that can be fixed by asking for an additional approval
        // from `sender`, but let's keep it simple for now.
        exchange_for: ID,
        /// the escrowed object that we store into an option because it could already be taken
        escrowed: Option<T>,
    }

    // Error codes
    /// The `sender` and `recipient` of the two escrowed objects do not match
    const EMismatchedSenderRecipient: u64 = 0;
    /// The `exchange_for` fields of the two escrowed objects do not match
    const EMismatchedExchangeObject: u64 = 1;
    /// The escrow has already been exchanged or cancelled
    const EAlreadyExchangedOrCancelled: u64 = 3;

    public fun create<T: key + store>(
        recipient: address,
        exchange_for: ID,
        escrowed_item: T,
        ctx: &mut TxContext
    ) {
        let sender = tx_context::sender(ctx);
        let id = object::new(ctx);
        let escrowed = option::some(escrowed_item);

        transfer::public_share_object(
            Exchange<T> {
                id, sender, recipient, exchange_for, escrowed
            }
        );
    }

    /// The `recipient` of the escrow can exchange `obj` with the escrowed item
    public fun exchange<T: key + store, ExchangeForT: key + store>(
        obj: ExchangeForT,
        escrow: &mut Exchange<T>,
        ctx: &TxContext
    ) {
        assert!(option::is_some(&escrow.escrowed), EAlreadyExchangedOrCancelled);
        let escrowed_item = option::extract<T>(&mut escrow.escrowed);
        assert!(&tx_context::sender(ctx) == &escrow.recipient, EMismatchedSenderRecipient);
        assert!(object::borrow_id(&obj) == &escrow.exchange_for, EMismatchedExchangeObject);
        // everything matches. do the swap!
        transfer::public_transfer(escrowed_item, tx_context::sender(ctx));
        transfer::public_transfer(obj, escrow.sender);
    }

    /// The `creator` can cancel the escrow and get back the escrowed item
    public fun cancel<T: key + store>(
        escrow: &mut Exchange<T>,
        ctx: &TxContext
    ): T {
        assert!(&tx_context::sender(ctx) == &escrow.sender, EMismatchedSenderRecipient);
        assert!(option::is_some(&escrow.escrowed), EAlreadyExchangedOrCancelled);
        option::extract<T>(&mut escrow.escrowed)
    }

    // === Tests ===
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    #[test_only] use sui::test_scenario::{Self, Scenario};

    #[test_only]
    fun test_coin(ts: &mut Scenario): Coin<SUI> {
        coin::mint_for_testing<SUI>(42, test_scenario::ctx(ts))
    }

    #[test]
    fun test_successful_swap() {
        let ts = test_scenario::begin(@0x0);
        let alice = @0xA;
        let bob = @0xB;

        // Initialize coins for Alice + Bob
        let (alice_coin_id) = {
            test_scenario::next_tx(&mut ts, alice);
            let c = test_coin(&mut ts);
            let alice_coin_id = (object::id(&c));
            transfer::public_transfer(c, alice);
            alice_coin_id
        };

        let (bob_coin_id) = {
            test_scenario::next_tx(&mut ts, bob);
            let c = test_coin(&mut ts);
            let bob_coin_id = (object::id(&c));
            transfer::public_transfer(c, bob);
            bob_coin_id
        };
        // Alice creates a public Exchange asking for an object from bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let a_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            create<Coin<SUI>>(
                bob,
                bob_coin_id,
                a_coin,
                test_scenario::ctx(&mut ts)
            );
        };

        // Bob pulls the shared object and exchanges and transfers to himself
        {
            test_scenario::next_tx(&mut ts, bob);
            let exchange_obj = test_scenario::take_shared<Exchange<Coin<SUI>>>(&mut ts);
            let b_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            exchange<Coin<SUI>, Coin<SUI>>(b_coin, &mut exchange_obj, test_scenario::ctx(&mut ts));
            test_scenario::return_shared(exchange_obj);
        };

        // Commit effects from the swap
        test_scenario::next_tx(&mut ts, @0x0);

        // Alice gets the object from Bob
        {
            let b_c: Coin<SUI> = test_scenario::take_from_address_by_id(&ts, alice, bob_coin_id);
            test_scenario::return_to_address(alice, b_c);
        };

        // Bob gets the object from Alice
        {
            let c: Coin<SUI> = test_scenario::take_from_address_by_id(&ts, bob, alice_coin_id);
            test_scenario::return_to_address(bob, c);
        };

        test_scenario::end(ts);
    }


    #[test]
    #[expected_failure(abort_code = EMismatchedExchangeObject)]
    fun test_mismatch_object() {
        let ts = test_scenario::begin(@0x0);
        let alice = @0xA;
        let bob = @0xB;

        // Initialize coins for Alice + Bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let c = test_coin(&mut ts);
            transfer::public_transfer(c, alice);
        };
        {
            test_scenario::next_tx(&mut ts, bob);
            let c = test_coin(&mut ts);
            transfer::public_transfer(c, bob);
        };

        // Alice creates a public Exchange asking for an object from bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let mock_want = test_coin(&mut ts);
            let mock_want_id = (object::id(&mock_want));
            let a_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            create<Coin<SUI>>(
                bob,
                mock_want_id,
                a_coin,
                test_scenario::ctx(&mut ts)
            );
            transfer::public_transfer(mock_want, alice);
        };
        // Bob tries to swap his object for Alice's swap
        {
            test_scenario::next_tx(&mut ts, bob);
            let exchange_obj = test_scenario::take_shared<Exchange<Coin<SUI>>>(&mut ts);
            let b_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            exchange<Coin<SUI>, Coin<SUI>>(b_coin, &mut exchange_obj, test_scenario::ctx(&mut ts));
            test_scenario::return_shared(exchange_obj);
        };

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedSenderRecipient)]
    fun test_mismatch_sender() {
        let ts = test_scenario::begin(@0x0);
        let alice = @0xA;
        let bob = @0xB;
        let cat = @0xC;

        // Initialize coins for Alice + Bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let c = test_coin(&mut ts);
            transfer::public_transfer(c, alice);
        };
        let (bob_coin_id) = {
            test_scenario::next_tx(&mut ts, bob);
            let c = test_coin(&mut ts);
            let bob_coin_id = (object::id(&c));
            transfer::public_transfer(c, bob);
            bob_coin_id
        };

        // Alice creates a public Exchange asking for an object from bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let a_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            create<Coin<SUI>>(
                cat,
                bob_coin_id,
                a_coin,
                test_scenario::ctx(&mut ts)
            );
        };
        // Bob tries to swap his object for Alice's swap
        {
            test_scenario::next_tx(&mut ts, bob);
            let exchange_obj = test_scenario::take_shared<Exchange<Coin<SUI>>>(&mut ts);
            let b_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            exchange<Coin<SUI>, Coin<SUI>>(b_coin, &mut exchange_obj, test_scenario::ctx(&mut ts));
            test_scenario::return_shared(exchange_obj);
        };

        abort 1337
    }

    #[test]
    fun test_return_to_sender() {
        let ts = test_scenario::begin(@0x0);
        let alice = @0xA;
        let bob = @0xB;

        // Initialize coins for Alice + Bob
        test_scenario::next_tx(&mut ts, alice);
        let c = test_coin(&mut ts);
        transfer::public_transfer(c, alice);

        let (bob_coin_id) = {
            test_scenario::next_tx(&mut ts, bob);
            let c = test_coin(&mut ts);
            let bob_coin_id = (object::id(&c));
            transfer::public_transfer(c, bob);
            bob_coin_id
        };
        // Alice requests to swap with Bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let a_coin: Coin<SUI> = test_scenario::take_from_sender(&ts);
            create<Coin<SUI>>(
                bob,
                bob_coin_id,
                a_coin,
                test_scenario::ctx(&mut ts)
            );
        };

        // Alice decides to cancel a swap with Bob
        {
            test_scenario::next_tx(&mut ts, alice);
            let exchange_obj = test_scenario::take_shared<Exchange<Coin<SUI>>>(&mut ts);
            let alice_item = cancel<Coin<SUI>>(
                &mut exchange_obj,
                test_scenario::ctx(&mut ts)
            );
            test_scenario::return_shared(exchange_obj);
            transfer::public_transfer(alice_item, alice);
        };
        test_scenario::end(ts);
    }
}