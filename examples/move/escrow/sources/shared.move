// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects using shared objects without a trusted
/// third party.
///
/// The protocol consists of three phases:
///
/// 1. One party `lock`s their object, getting a `Locked` object and its `Key`.
///    This party can `unlock` their object to preserve livness if the other
///    party stalls before completing the second stage.
///
/// 2. The other party registers a publicly accessible, shared `Escrow` object.
///    This effectively locks their object at a particular version as well,
///    waiting for the first party to complete the swap.  The second party is
///    able to request their object is returned to them, to preserve liveness as
///    well.
///
/// 3. The first party sends their locked object and its key to the shared
///    `Escrow` object.  This completes the swap, as long as all conditions are
///    met:
///
///    - The sender of the swap transaction is the recipient of the `Escrow`.
///
///    - The key of the desired object (`exchange_key`) in the escrow matches
///      the key supplied in the swap.
///
///    - The key supplied in the swap unlocks the `Locked<U>`.
module escrow::shared {
    use std::option::{Self, Option};

    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    use escrow::lock::{Self, Locked, Key};

    /// An object held in escrow
    struct Escrow<T: key + store> has key, store {
        id: UID,

        /// Owner of `escrowed`
        sender: address,

        /// Intended recipient
        recipient: address,

        /// ID of the key that opens the lock on the object sender wants from
        /// recipient.
        exchange_key: ID,

        /// the escrowed object that we store into an option because it could
        /// already be taken
        escrowed: Option<T>,
    }

    // === Error codes ===

    /// The `sender` and `recipient` of the two escrowed objects do not match
    const EMismatchedSenderRecipient: u64 = 0;

    /// The `exchange_for` fields of the two escrowed objects do not match
    const EMismatchedExchangeObject: u64 = 1;

    /// The escrow has already been exchanged or returned to the original sender
    const EAlreadyExchangedOrReturned: u64 = 2;

    // === Public Functions ===

    public fun create<T: key + store>(
        escrowed: T,
        exchange_key: ID,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let escrow = Escrow {
            id: object::new(ctx),
            sender: tx_context::sender(ctx),
            recipient,
            exchange_key,
            escrowed: option::some(escrowed),
        };

        transfer::public_share_object(escrow);
    }

    /// The `recipient` of the escrow can exchange `obj` with the escrowed item
    public fun swap<T: key + store, U: key + store>(
        escrow: &mut Escrow<T>,
        key: Key,
        locked: Locked<U>,
        ctx: &TxContext,
    ): T {
        assert!(option::is_some(&escrow.escrowed), EAlreadyExchangedOrReturned);
        assert!(escrow.recipient == tx_context::sender(ctx), EMismatchedSenderRecipient);
        assert!(escrow.exchange_key == object::id(&key), EMismatchedExchangeObject);

        let escrowed1 = option::extract<T>(&mut escrow.escrowed);
        let escrowed2 = lock::unlock(locked, key);

        // Do the actual swap
        transfer::public_transfer(escrowed2, escrow.sender);
        escrowed1
    }

    /// The `creator` can cancel the escrow and get back the escrowed item
    public fun return_to_sender<T: key + store>(
        escrow: &mut Escrow<T>,
        ctx: &TxContext
    ): T {
        assert!(escrow.sender == tx_context::sender(ctx), EMismatchedSenderRecipient);
        assert!(option::is_some(&escrow.escrowed), EAlreadyExchangedOrReturned);
        option::extract<T>(&mut escrow.escrowed)
    }

    // === Tests ===
    #[test_only] use sui::coin::{Self, Coin};
    #[test_only] use sui::sui::SUI;
    #[test_only] use sui::test_scenario::{Self as ts, Scenario};

    #[test_only] const ALICE: address = @0xA;
    #[test_only] const BOB: address = @0xB;
    #[test_only] const DIANE: address = @0xD;

    #[test_only]
    fun test_coin(ts: &mut Scenario): Coin<SUI> {
        coin::mint_for_testing<SUI>(42, ts::ctx(ts))
    }

    #[test]
    fun test_successful_swap() {
        let ts = ts::begin(@0x0);

        // Bob locks the object they want to trade.
        let (i2, ik2) = {
            ts::next_tx(&mut ts, BOB);
            let c = test_coin(&mut ts);
            let cid = object::id(&c);
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));
            let kid = object::id(&k);
            transfer::public_transfer(l, BOB);
            transfer::public_transfer(k, BOB);
            (cid, kid)
        };

        // Alice creates a public Escrow holding the object they are willing to
        // share, and the object they want from Bob
        let i1 = {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            let cid = object::id(&c);
            create(c, ik2, BOB, ts::ctx(&mut ts));
            cid
        };

        // Bob responds by offering their object, and gets Alice's object in
        // return.
        {
            ts::next_tx(&mut ts, BOB);
            let escrow = ts::take_shared(&ts);
            let k2: Key = ts::take_from_sender(&ts);
            let l2: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                &mut escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
            ts::return_shared(escrow);
        };

        // Commit effects from the swap
        ts::next_tx(&mut ts, @0x0);

        // Alice gets the object from Bob
        {
            let c: Coin<SUI> = ts::take_from_address_by_id(&ts, ALICE, i2);
            ts::return_to_address(ALICE, c);
        };

        // Bob gets the object from Alice
        {
            let c: Coin<SUI> = ts::take_from_address_by_id(&ts, BOB, i1);
            ts::return_to_address(BOB, c);
        };

        ts::end(ts);
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedSenderRecipient)]
    fun test_mismatch_sender() {
        let ts = ts::begin(@0x0);

        let ik2 = {
            ts::next_tx(&mut ts, DIANE);
            let c = test_coin(&mut ts);
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));
            let kid = object::id(&k);
            transfer::public_transfer(l, DIANE);
            transfer::public_transfer(k, DIANE);
            kid
        };

        // Alice wants to trade with Bob.
        {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            create(c, ik2, BOB, ts::ctx(&mut ts));
        };

        // But Diane is the one who attempts the swap
        {
            ts::next_tx(&mut ts, DIANE);
            let escrow = ts::take_shared(&ts);
            let k2: Key = ts::take_from_sender(&ts);
            let l2: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                &mut escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, DIANE);
        };

        abort 1337
    }


    #[test]
    #[expected_failure(abort_code = EMismatchedExchangeObject)]
    fun test_mismatch_object() {
        let ts = ts::begin(@0x0);

        {
            ts::next_tx(&mut ts, BOB);
            let c = test_coin(&mut ts);
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));
            transfer::public_transfer(l, BOB);
            transfer::public_transfer(k, BOB);
        };

        // Alice wants to trade with Bob, but Alice has asked for an object (via
        // its `exchange_key`) that Bob has not put up for the swap.
        {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            let cid = object::id(&c);
            create(c, cid, BOB, ts::ctx(&mut ts));
        };

        // When Bob tries to complete the swap, it will fail, because they
        // cannot meet Alice's requirements.
        {
            ts::next_tx(&mut ts, BOB);
            let escrow = ts::take_shared(&ts);
            let k2: Key = ts::take_from_sender(&ts);
            let l2: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                &mut escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
        };

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedExchangeObject)]
    fun test_object_tamper() {
        let ts = ts::begin(@0x0);

        // Bob locks their object.
        let ik2 = {
            ts::next_tx(&mut ts, BOB);
            let c = test_coin(&mut ts);
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));
            let kid = object::id(&k);
            transfer::public_transfer(l, BOB);
            transfer::public_transfer(k, BOB);
            kid
        };

        // Alice sets up the escrow
        {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            create(c, ik2, BOB, ts::ctx(&mut ts));
        };

        // Bob has a change of heart, so they unlock the object and tamper with
        // it before initiating the swap, but it won't be possible for Bob to
        // hide their tampering.
        {
            ts::next_tx(&mut ts, BOB);
            let k: Key = ts::take_from_sender(&ts);
            let l: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = lock::unlock(l, k);

            let _dust = coin::split(&mut c, 1, ts::ctx(&mut ts));
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));

            let escrow = ts::take_shared(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                &mut escrow,
                k,
                l,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
        };

        abort 1337
    }

    #[test]
    fun test_return_to_sender() {
        let ts = ts::begin(@0x0);

        // Alice puts up the object they want to trade
        let cid = {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            let cid = object::id(&c);
            let i = object::id_from_address(@0x0);
            create(c, i, BOB, ts::ctx(&mut ts));
            cid
        };

        // ...but has a change of heart and takes it back
        {
            ts::next_tx(&mut ts, ALICE);
            let escrow = ts::take_shared(&ts);
            let c = return_to_sender<Coin<SUI>>(&mut escrow, ts::ctx(&mut ts));

            transfer::public_transfer(c, ALICE);
            ts::return_shared(escrow);
        };

        ts::next_tx(&mut ts, @0x0);

        // Alice can then access it.
        {
            let c: Coin<SUI> = ts::take_from_address_by_id(&ts, ALICE, cid);
            ts::return_to_address(ALICE, c)
        };

        ts::end(ts);
    }

    #[test]
    #[expected_failure(abort_code = EAlreadyExchangedOrReturned)]
    fun test_return_to_sender_failed_swap() {
        let ts = ts::begin(@0x0);

        // Bob locks their object.
        let ik2 = {
            ts::next_tx(&mut ts, BOB);
            let c = test_coin(&mut ts);
            let (l, k) = lock::lock(c, ts::ctx(&mut ts));
            let kid = object::id(&k);
            transfer::public_transfer(l, BOB);
            transfer::public_transfer(k, BOB);
            kid
        };

        // Alice creates a public Escrow holding the object they are willing to
        // share, and the object they want from Bob
        {
            ts::next_tx(&mut ts, ALICE);
            let c = test_coin(&mut ts);
            create(c, ik2, BOB, ts::ctx(&mut ts));
        };

        // ...but then has a change of heart
        {
            ts::next_tx(&mut ts, ALICE);
            let escrow = ts::take_shared(&ts);
            let c = return_to_sender<Coin<SUI>>(&mut escrow, ts::ctx(&mut ts));
            transfer::public_transfer(c, ALICE);
            ts::return_shared(escrow);
        };

        // Bob's attempt to complete the swap will now fail.
        {
            ts::next_tx(&mut ts, BOB);
            let escrow = ts::take_shared(&ts);
            let k2: Key = ts::take_from_sender(&ts);
            let l2: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                &mut escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
        };

        abort 1337
    }
}
