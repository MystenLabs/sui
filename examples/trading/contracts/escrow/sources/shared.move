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
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use sui::event;
    use sui::dynamic_object_field::{Self as dof};

    use escrow::lock::{Self, Locked, Key};

    /// The `name` of the DOF that holds the Escrowed object.
    /// Allows easy discoverability for the escrowed object.
    struct EscrowedObjectKey has copy, store, drop {}

    /// An object held in escrow
    /// 
    /// The escrowed object is added as a Dynamic Object Field so it can still be looked-up.
    struct Escrow<phantom T: key + store> has key, store {
        id: UID,

        /// Owner of `escrowed`
        sender: address,

        /// Intended recipient
        recipient: address,

        /// ID of the key that opens the lock on the object sender wants from
        /// recipient.
        exchange_key: ID,
    }

    // === Error codes ===

    /// The `sender` and `recipient` of the two escrowed objects do not match
    const EMismatchedSenderRecipient: u64 = 0;

    /// The `exchange_for` fields of the two escrowed objects do not match
    const EMismatchedExchangeObject: u64 = 1;

    // === Public Functions ===

    public fun create<T: key + store>(
        escrowed: T,
        exchange_key: ID,
        recipient: address,
        ctx: &mut TxContext
    ) {
        let escrow = Escrow<T> {
            id: object::new(ctx),
            sender: tx_context::sender(ctx),
            recipient,
            exchange_key,
        };

        event::emit(EscrowCreated {
            escrow_id: object::id(&escrow),
            key_id: exchange_key,
            sender: escrow.sender,
            recipient,
            item_id: object::id(&escrowed),
        });

        dof::add(&mut escrow.id, EscrowedObjectKey {}, escrowed);

        transfer::public_share_object(escrow);
    }

    /// The `recipient` of the escrow can exchange `obj` with the escrowed item
    public fun swap<T: key + store, U: key + store>(
        escrow: Escrow<T>,
        key: Key,
        locked: Locked<U>,
        ctx: &TxContext,
    ): T {
        let escrowed = dof::remove<EscrowedObjectKey, T>(&mut escrow.id, EscrowedObjectKey {});

        let Escrow {
            id,
            sender,
            recipient,
            exchange_key,
        } = escrow;

        assert!(recipient == tx_context::sender(ctx), EMismatchedSenderRecipient);
        assert!(exchange_key == object::id(&key), EMismatchedExchangeObject);

        // Do the actual swap
        transfer::public_transfer(lock::unlock(locked, key), sender);

        event::emit(EscrowSwapped {
            escrow_id: object::uid_to_inner(&id),
        });

        object::delete(id);

        escrowed
    }

    /// The `creator` can cancel the escrow and get back the escrowed item
    public fun return_to_sender<T: key + store>(
        escrow: Escrow<T>,
        ctx: &TxContext
    ): T {

        event::emit(EscrowCancelled {
            escrow_id: object::id(&escrow)
        });

        let escrowed = dof::remove<EscrowedObjectKey, T>(&mut escrow.id, EscrowedObjectKey {});

        let Escrow {
            id,
            sender,
            recipient: _,
            exchange_key: _,
        } = escrow;

        assert!(sender == tx_context::sender(ctx), EMismatchedSenderRecipient);
        object::delete(id);
        escrowed
    }

    // === Events ===
    struct EscrowCreated has copy, drop {
        /// the ID of the escrow that was created
        escrow_id: ID,
        /// The ID of the `Key` that unlocks the requested object.
        key_id: ID,
        /// The id of the sender who'll receive `T` upon swap
        sender: address,
        /// The (original) recipient of the escrowed object
        recipient: address,
        /// The ID of the escrowed item
        item_id: ID,
    }

    struct EscrowSwapped has copy, drop {
        escrow_id: ID
    }

    struct EscrowCancelled has copy, drop {
        escrow_id: ID
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
                escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
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
                escrow,
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
                escrow,
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
                escrow,
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
            let c = return_to_sender<Coin<SUI>>(escrow, ts::ctx(&mut ts));

            transfer::public_transfer(c, ALICE);
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
    #[expected_failure]
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
            let c = return_to_sender<Coin<SUI>>(escrow, ts::ctx(&mut ts));
            transfer::public_transfer(c, ALICE);
        };

        // Bob's attempt to complete the swap will now fail.
        {
            ts::next_tx(&mut ts, BOB);
            let escrow = ts::take_shared(&ts);
            let k2: Key = ts::take_from_sender(&ts);
            let l2: Locked<Coin<SUI>> = ts::take_from_sender(&ts);
            let c = swap<Coin<SUI>, Coin<SUI>>(
                escrow,
                k2,
                l2,
                ts::ctx(&mut ts),
            );

            transfer::public_transfer(c, BOB);
        };

        abort 1337
    }
}
