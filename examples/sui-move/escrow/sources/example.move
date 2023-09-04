// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects that trusts a third party for liveness,
/// but not safety.
///
/// Swap via Escrow proceeds in four phases:
///
/// 1. Both parties `lock` their objects, getting the `Locked` object and a
///    `Key`.  Each party can `unlock` their object, to preserve liveness if the
///    other party stalls before completing the second stage.
///
/// 2. Both parties register an `Escrow` object with the custodian, holding
///    their locked object and signaling their interest in the other party's
///    object by referencing the key that's locking it.  They keep their
///    respective keys.  The custodan is trusted to preserve liveness.
///
/// 3. The custodian swaps the locked objects, but also swaps their keys,
///    i.e. if party A with key K locking O successfully swaps with party B and
///    key L locking P, then the custodian returns object P locked by K to A,
///    and object O locked by key L to B.
///
///    A safe (successful) swap requires checking that the exchange key for O's
///    escrow is L and vice versa for P and K.  If this is not true, it means
///    the wrong objects are being swapped, either because the custodian paired
///    the wrong escrows together, or because one of the parties tampered with
///    their object after locking it.
///
/// 4. Each party can unlock the other party's object with their own key.
module escrow::example {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// A wrapper that protects access to `obj` by requiring access to a `Key`.
    ///
    /// Used to ensure an object is not modified if it might be involved in a
    /// swap.
    struct Locked<T: key + store> has key, store {
        id: UID,
        key: ID,
        swapped: bool,
        obj: T,
    }

    /// Key to open a locked object (consuming the `Key`)
    struct Key has key, store { id: UID }

    /// An object held in escrow
    struct Escrow<T: key + store> has key {
        id: UID,

        /// Owner of `escrowed`
        sender: address,

        /// Intended recipient
        recipient: address,

        /// The ID of the key that opens the lock on the object sender wants
        /// from recipient.
        exchange_key: ID,

        /// The escrowed object.
        escrowed: Locked<T>,
    }

    // === Error codes ===

    /// The key does not match this lock.
    const ELockKeyMismatch: u64 = 0;

    /// The locked object has already been swapped
    const EAlreadySwapped: u64 = 1;

    /// The `sender` and `recipient` of the two escrowed objects do not match
    const EMismatchedSenderRecipient: u64 = 2;

    /// The `exchange_for` fields of the two escrowed objects do not match
    const EMismatchedExchangeObject: u64 = 3;

    // === Public Functions ===

    /// Lock `obj` and get a key that can be used to unlock it.
    public fun lock<T: key + store>(
        obj: T,
        ctx: &mut TxContext,
    ): (Locked<T>, Key) {
        let key = Key { id: object::new(ctx) };
        let lock = Locked {
            id: object::new(ctx),
            key: object::id(&key),
            obj,
            swapped: false,
        };
        (lock, key)
    }

    /// Unlock the object in `locked`, consuming the `key`.  Fails if the wrong
    /// `key` is passed in for the locked object.
    public fun unlock<T: key + store>(locked: Locked<T>, key: Key): T {
        assert!(locked.key == object::id(&key), ELockKeyMismatch);
        let Key { id } = key;
        object::delete(id);

        let Locked { id, key: _, swapped: _, obj } = locked;
        object::delete(id);
        obj
    }

    /// `tx_context::sender(ctx)` requests a swap with `recipient` of a locked
    /// object `escrowed` in exchange for an object referred to by
    /// `exchange_key`.  The swap is performed by a third-party, `custodian`,
    /// that is trusted to maintain liveness, but not safety (the only actions
    /// they can perform are to successfully progress the swap).
    ///
    /// `exchange_key` is the ID of a `Key` that unlocks the sender's desired
    /// object.  Gating the swap on the key ensures that it will not succeed if
    /// the desired object is tampered with after the sender's object is held in
    /// escrow, because the recipient would have to consume the key to tamper
    /// with the object, and if they re-locked the object it would be protected
    /// by a different, incompatible key.
    public fun create<T: key + store>(
        recipient: address,
        custodian: address,
        exchange_key: ID,
        escrowed: Locked<T>,
        ctx: &mut TxContext,
    ) {
        assert!(!escrowed.swapped, EAlreadySwapped);

        let escrow = Escrow {
            id: object::new(ctx),
            sender: tx_context::sender(ctx),
            recipient,
            exchange_key,
            escrowed,
        };

        transfer::transfer(escrow, custodian);
    }

    /// Function for custodian (trusted third-party) to perform a swap between
    /// two parties.  Fails if their senders and recipients do not match, or if
    /// their respective desired objects do not match.
    public fun swap<T: key + store, U: key + store>(
        obj1: Escrow<T>,
        obj2: Escrow<U>,
    ) {
        let Escrow {
            id: id1,
            sender: sender1,
            recipient: recipient1,
            exchange_key: exchange_key1,
            escrowed: escrowed1,
        } = obj1;

        let Escrow {
            id: id2,
            sender: sender2,
            recipient: recipient2,
            exchange_key: exchange_key2,
            escrowed: escrowed2,
        } = obj2;

        object::delete(id1);
        object::delete(id2);

        // Make sure the sender and recipient match each other
        assert!(sender1 == recipient2, EMismatchedSenderRecipient);
        assert!(sender2 == recipient1, EMismatchedSenderRecipient);

        // Make sure the objects match each other and haven't been modified
        // (they remain locked).
        assert!(escrowed1.key == exchange_key2, EMismatchedExchangeObject);
        assert!(escrowed2.key == exchange_key1, EMismatchedExchangeObject);

        // Swap keys on the locks so that each recipient can use the key they
        // have to unlock the object they receive.
        let tmp = escrowed1.key;
        escrowed1.key = escrowed2.key;
        escrowed2.key = tmp;

        // Mark the locked objects as swapped so they don't get swapped again.
        escrowed1.swapped = true;
        escrowed2.swapped = true;

        // Do the actual swap
        transfer::public_transfer(escrowed1, recipient1);
        transfer::public_transfer(escrowed2, recipient2);
    }

    /// The custodian can always return an escrowed object to its original
    /// owner.
    public fun return_to_sender<T: key + store>(obj: Escrow<T>) {
        let Escrow {
            id,
            sender,
            recipient: _,
            exchange_key: _,
            escrowed,
        } = obj;

        object::delete(id);
        transfer::public_transfer(escrowed, sender);
    }

    // === Tests ===
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;
    use sui::test_scenario::{Self as ts, Scenario};

    #[test_only]
    fun test_coin(ts: &mut Scenario): Coin<SUI> {
        coin::mint_for_testing<SUI>(42, ts::ctx(ts))
    }

    #[test]
    fun test_lock_unlock() {
        let ts = ts::begin(@0xA);
        let coin = test_coin(&mut ts);

        let (lock, key) = lock(coin, ts::ctx(&mut ts));
        let coin = unlock(lock, key);

        coin::burn_for_testing(coin);
        ts::end(ts);
    }

    #[test]
    fun test_successful_swap() {
        // Party A locks the object they want to trade
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let i1 = object::id(&c1);
        let (l1, k1) = lock(c1, ts::ctx(&mut ts));

        // Party B locks their object as well.
        ts::next_tx(&mut ts, @0xB);
        let c2 = test_coin(&mut ts);
        let i2 = object::id(&c2);
        let (l2, k2) = lock(c2, ts::ctx(&mut ts));

        // Party A gives Party C (the custodian) their object to hold in escrow.
        ts::next_tx(&mut ts, @0xA);
        create(@0xB, @0xC, object::id(&k2), l1, ts::ctx(&mut ts));

        // Party B does the same.
        ts::next_tx(&mut ts, @0xB);
        create(@0xA, @0xC, object::id(&k1), l2, ts::ctx(&mut ts));

        // The Custodian makes the swap
        ts::next_tx(&mut ts, @0xC);
        swap<Coin<SUI>, Coin<SUI>>(
            ts::take_from_sender(&mut ts),
            ts::take_from_sender(&mut ts),
        );

        // Party A unlocks the object from B
        ts::next_tx(&mut ts, @0xA);
        let c2 = unlock<Coin<SUI>>(ts::take_from_sender(&mut ts), k1);
        assert!(object::id(&c2) == i2, 0);

        // Party B unlocks the object from A
        ts::next_tx(&mut ts, @0xB);
        let c1 = unlock<Coin<SUI>>(ts::take_from_sender(&mut ts), k2);
        assert!(object::id(&c1) == i1, 0);

        coin::burn_for_testing(c1);
        coin::burn_for_testing(c2);
        ts::end(ts);
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedSenderRecipient)]
    fun test_mismatch_sender() {
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let (l1, k1) = lock(c1, ts::ctx(&mut ts));

        ts::next_tx(&mut ts, @0xB);
        let c2 = test_coin(&mut ts);
        let (l2, k2) = lock(c2, ts::ctx(&mut ts));

        // A wants to trade with B.
        ts::next_tx(&mut ts, @0xA);
        create(@0xB, @0xC, object::id(&k2), l1, ts::ctx(&mut ts));

        // But B wants to trade with F.
        ts::next_tx(&mut ts, @0xB);
        create(@0xF, @0xC, object::id(&k1), l2, ts::ctx(&mut ts));

        // When the custodian tries to match up the swap, it will fail.
        ts::next_tx(&mut ts, @0xC);
        swap<Coin<SUI>, Coin<SUI>>(
            ts::take_from_sender(&mut ts),
            ts::take_from_sender(&mut ts),
        );

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedExchangeObject)]
    fun test_mismatch_object() {
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let (l1, k1) = lock(c1, ts::ctx(&mut ts));

        ts::next_tx(&mut ts, @0xB);
        let c2 = test_coin(&mut ts);
        let (l2, _k2) = lock(c2, ts::ctx(&mut ts));

        // A wants to trade with B, but A has asked for an object (via its
        // `exchange_key`) that B has not put up for the swap.
        ts::next_tx(&mut ts, @0xA);
        create(@0xB, @0xC, object::id(&k1), l1, ts::ctx(&mut ts));

        ts::next_tx(&mut ts, @0xB);
        create(@0xA, @0xC, object::id(&k1), l2, ts::ctx(&mut ts));

        // When the custodian tries to match up the swap, it will fail.
        ts::next_tx(&mut ts, @0xC);
        swap<Coin<SUI>, Coin<SUI>>(
            ts::take_from_sender(&mut ts),
            ts::take_from_sender(&mut ts),
        );

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EMismatchedExchangeObject)]
    fun test_object_tamper() {
        // Party A locks the object they want to trade
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let (l1, k1) = lock(c1, ts::ctx(&mut ts));

        // Party B locks their object as well.
        ts::next_tx(&mut ts, @0xB);
        let c2 = test_coin(&mut ts);
        let (l2, k2) = lock(c2, ts::ctx(&mut ts));

        // Party A gives Party C (the custodian) their object to hold in escrow.
        ts::next_tx(&mut ts, @0xA);
        create(@0xB, @0xC, object::id(&k2), l1, ts::ctx(&mut ts));

        // Party B has a change of heart, so they unlock the object and tamper
        // with it.
        ts::next_tx(&mut ts, @0xB);
        let c2 = unlock(l2, k2);
        let _c = coin::split(&mut c2, 1, ts::ctx(&mut ts));

        // They try and hide their tracks be re-locking the same coin.
        let (l2, _k) = lock(c2, ts::ctx(&mut ts));
        create(@0xA, @0xC, object::id(&k1), l2, ts::ctx(&mut ts));

        // When the Custodian makes the swap, we detect B's nefarious behaviour.
        ts::next_tx(&mut ts, @0xC);
        swap<Coin<SUI>, Coin<SUI>>(
            ts::take_from_sender(&mut ts),
            ts::take_from_sender(&mut ts),
        );

        abort 1337
    }

    #[test]
    #[expected_failure(abort_code = EAlreadySwapped)]
    fun test_double_swap() {
        // Party A locks the object they want to trade
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let (l1, _k) = lock(c1, ts::ctx(&mut ts));

        // Party B locks their object as well.
        ts::next_tx(&mut ts, @0xB);
        let c2 = test_coin(&mut ts);
        let (l2, k2) = lock(c2, ts::ctx(&mut ts));

        // Party A gives Party C (the custodian) their object to hold in escrow.
        ts::next_tx(&mut ts, @0xA);
        create(@0xB, @0xC, object::id(&k2), l1, ts::ctx(&mut ts));

        // Party F is colluding with B, and locks a dud coin, and sends that to
        // itself for "escrow".
        ts::next_tx(&mut ts, @0xF);
        let cz = coin::zero<SUI>(ts::ctx(&mut ts));
        let (lz, kz) = lock(cz, ts::ctx(&mut ts));
        create(@0xB, @0xF, object::id(&k2), lz, ts::ctx(&mut ts));

        // Party B sends the object they promised to A, to F instead.
        ts::next_tx(&mut ts, @0xB);
        create(@0xF, @0xF, object::id(&kz), l2, ts::ctx(&mut ts));

        // Party F pretends to be a custodian and swaps the dud coin with the
        // coin that B promised for A.
        ts::next_tx(&mut ts, @0xF);
        swap<Coin<SUI>, Coin<SUI>>(
            ts::take_from_sender(&mut ts),
            ts::take_from_sender(&mut ts),
        );

        // Party B now has the dud coin, locked by the key that A originally
        // referenced, so attempts to send that to the custodian, which will
        // fail.
        ts::next_tx(&mut ts, @0xB);
        let l2: Locked<Coin<SUI>> = ts::take_from_sender(&mut ts);

        assert!(coin::value(&l2.obj) == 0, 0);
        create(@0xA, @0xC, object::id(&k2), l2, ts::ctx(&mut ts));

        abort 1337
    }

    #[test]
    fun test_return_to_sender() {
        // Party A locks the object they want to trade
        let ts = ts::begin(@0xA);
        let c1 = test_coin(&mut ts);
        let i1 = object::id(&c1);
        let (l1, k1) = lock(c1, ts::ctx(&mut ts));
        create(@0xB, @0xC, object::id(&l1), l1, ts::ctx(&mut ts));

        // Custodian sends it back
        ts::next_tx(&mut ts, @0xC);
        return_to_sender<Coin<SUI>>(ts::take_from_sender(&mut ts));

        // Party A can then unlock it.
        ts::next_tx(&mut ts, @0xA);
        let c1 = unlock<Coin<SUI>>(ts::take_from_sender(&mut ts), k1);
        assert!(object::id(&c1) == i1, 0);

        coin::burn_for_testing(c1);
        ts::end(ts);
    }
}
