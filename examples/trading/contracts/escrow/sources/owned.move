// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An escrow for atomic swap of objects using single-owner transactions that
/// trusts a third party for liveness, but not safety.
///
/// Swap via Escrow proceeds in three phases:
///
/// 1. Both parties `lock` their objects, getting the `Locked` object and a
///    `Key`.  Each party can `unlock` their object, to preserve liveness if the
///    other party stalls before completing the second stage.
///
/// 2. Both parties register an `Escrow` object with the custodian, this
///    requires passing the locked object and its key.  The key is consumed to
///    unlock the object, but its ID is remembered so the custodian can ensure
///    the right objects being swapped.  The custodian is trusted to preserve
///    liveness.
///
/// 3. The custodian swaps the locked objects as long as all conditions are met:
///
///    - The sender of one Escrow is the recipient of the other and vice versa.
///      If this is not true, the custodian has incorrectly paired together this
///      swap.
///
///    - The key of the desired object (`exchange_key`) matches the key the
///      other object was locked with (`escrowed_key`) and vice versa.

///      If this is not true, it means the wrong objects are being swapped,
///      either because the custodian paired the wrong escrows together, or
///      because one of the parties tampered with their object after locking it.
///
///      The key in question is the ID of the `Key` object that unlocked the
///      `Locked` object that the respective objects resided in immediately
///      before being sent to the custodian.
module escrow::owned;

use escrow::lock::{Locked, Key};

/// An object held in escrow
public struct Escrow<T: key + store> has key {
    id: UID,
    /// Owner of `escrowed`
    sender: address,
    /// Intended recipient
    recipient: address,
    /// The ID of the key that opens the lock on the object sender wants
    /// from recipient.
    exchange_key: ID,
    /// The ID of the key that locked the escrowed object, before it was
    /// escrowed.
    escrowed_key: ID,
    /// The escrowed object.
    escrowed: T,
}

// === Error codes ===

/// The `sender` and `recipient` of the two escrowed objects do not match
const EMismatchedSenderRecipient: u64 = 0;

/// The `exchange_key` fields of the two escrowed objects do not match
const EMismatchedExchangeObject: u64 = 1;

// === Public Functions ===

/// `ctx.sender()` requests a swap with `recipient` of a locked
/// object `locked` in exchange for an object referred to by `exchange_key`.
/// The swap is performed by a third-party, `custodian`, that is trusted to
/// maintain liveness, but not safety (the only actions they can perform are
/// to successfully progress the swap).
///
/// `locked` will be unlocked with its corresponding `key` before being sent
/// to the custodian, but the underlying object is still not accessible
/// until after the swap has executed successfully, or the custodian returns
/// the object.
///
/// `exchange_key` is the ID of a `Key` that unlocks the sender's desired
/// object.  Gating the swap on the key ensures that it will not succeed if
/// the desired object is tampered with after the sender's object is held in
/// escrow, because the recipient would have to consume the key to tamper
/// with the object, and if they re-locked the object it would be protected
/// by a different, incompatible key.
public fun create<T: key + store>(
    key: Key,
    locked: Locked<T>,
    exchange_key: ID,
    recipient: address,
    custodian: address,
    ctx: &mut TxContext,
) {
    let escrow = Escrow {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        exchange_key,
        escrowed_key: object::id(&key),
        escrowed: locked.unlock(key),
    };

    transfer::transfer(escrow, custodian);
}

/// Function for custodian (trusted third-party) to perform a swap between
/// two parties.  Fails if their senders and recipients do not match, or if
/// their respective desired objects do not match.
public fun swap<T: key + store, U: key + store>(obj1: Escrow<T>, obj2: Escrow<U>) {
    let Escrow {
        id: id1,
        sender: sender1,
        recipient: recipient1,
        exchange_key: exchange_key1,
        escrowed_key: escrowed_key1,
        escrowed: escrowed1,
    } = obj1;

    let Escrow {
        id: id2,
        sender: sender2,
        recipient: recipient2,
        exchange_key: exchange_key2,
        escrowed_key: escrowed_key2,
        escrowed: escrowed2,
    } = obj2;
    id1.delete();
    id2.delete();

    // Make sure the sender and recipient match each other
    assert!(sender1 == recipient2, EMismatchedSenderRecipient);
    assert!(sender2 == recipient1, EMismatchedSenderRecipient);

    // Make sure the objects match each other and haven't been modified
    // (they remain locked).
    assert!(escrowed_key1 == exchange_key2, EMismatchedExchangeObject);
    assert!(escrowed_key2 == exchange_key1, EMismatchedExchangeObject);

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
        escrowed_key: _,
        escrowed,
    } = obj;
    id.delete();
    transfer::public_transfer(escrowed, sender);
}

// === Tests ===
#[test_only]
use sui::coin::{Self, Coin};
#[test_only]
use sui::sui::SUI;
#[test_only]
use sui::test_scenario::{Self as ts, Scenario};

#[test_only]
use escrow::lock;

#[test_only]
const ALICE: address = @0xA;
#[test_only]
const BOB: address = @0xB;
#[test_only]
const CUSTODIAN: address = @0xC;
#[test_only]
const DIANE: address = @0xD;

#[test_only]
fun test_coin(ts: &mut Scenario): Coin<SUI> {
    coin::mint_for_testing<SUI>(42, ts::ctx(ts))
}

#[test]
fun test_successful_swap() {
    let mut ts = ts::begin(@0x0);

    // Alice locks the object they want to trade
    let (i1, ik1) = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, ALICE);
        transfer::public_transfer(k, ALICE);
        (cid, kid)
    };

    // Bob locks their object as well.
    let (i2, ik2) = {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
        (cid, kid)
    };

    // Alice gives the custodian their object to hold in escrow.
    {
        ts.next_tx(ALICE);
        let k1: Key = ts.take_from_sender();
        let l1: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k1, l1, ik2, BOB, CUSTODIAN, ts.ctx());
    };

    // Bob does the same.
    {
        ts.next_tx(BOB);
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k2, l2, ik1, ALICE, CUSTODIAN, ts.ctx());
    };

    // The custodian makes the swap
    {
        ts.next_tx(CUSTODIAN);
        swap<Coin<SUI>, Coin<SUI>>(
            ts.take_from_sender(),
            ts.take_from_sender(),
        );
    };

    // Commit effects from the swap
    ts.next_tx(@0x0);

    // Alice gets the object from Bob
    {
        let c: Coin<SUI> = ts.take_from_address_by_id(ALICE, i2);
        ts::return_to_address(ALICE, c);
    };

    // Bob gets the object from Alice
    {
        let c: Coin<SUI> = ts.take_from_address_by_id(BOB, i1);
        ts::return_to_address(BOB, c);
    };

    ts.end();
}

#[test]
#[expected_failure(abort_code = EMismatchedSenderRecipient)]
fun test_mismatch_sender() {
    let mut ts = ts::begin(@0x0);

    let ik1 = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, ALICE);
        transfer::public_transfer(k, ALICE);
        kid
    };

    let ik2 = {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
        kid
    };

    // Alice wants to trade with Bob.
    {
        ts.next_tx(ALICE);
        let k1: Key = ts.take_from_sender();
        let l1: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k1, l1, ik2, BOB, CUSTODIAN, ts.ctx());
    };

    // But Bob wants to trade with Diane.
    {
        ts.next_tx(BOB);
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k2, l2, ik1, DIANE, CUSTODIAN, ts.ctx());
    };

    // When the custodian tries to match up the swap, it will fail.
    {
        ts.next_tx(CUSTODIAN);
        swap<Coin<SUI>, Coin<SUI>>(
            ts.take_from_sender(),
            ts.take_from_sender(),
        );
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EMismatchedExchangeObject)]
fun test_mismatch_object() {
    let mut ts = ts::begin(@0x0);

    let ik1 = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, ALICE);
        transfer::public_transfer(k, ALICE);
        kid
    };

    {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
    };

    // Alice wants to trade with Bob, but Alice has asked for an
    // object (via its `exchange_key`) that Bob has not put up for
    // the swap.
    {
        ts.next_tx(ALICE);
        let k1: Key = ts.take_from_sender();
        let l1: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k1, l1, ik1, BOB, CUSTODIAN, ts.ctx());
    };

    {
        ts.next_tx(BOB);
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k2, l2, ik1, ALICE, CUSTODIAN, ts.ctx());
    };

    // When the custodian tries to match up the swap, it will fail.
    {
        ts.next_tx(CUSTODIAN);
        swap<Coin<SUI>, Coin<SUI>>(
            ts.take_from_sender(),
            ts.take_from_sender(),
        );
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EMismatchedExchangeObject)]
fun test_object_tamper() {
    let mut ts = ts::begin(@0x0);

    // Alice locks the object they want to trade
    let ik1 = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, ALICE);
        transfer::public_transfer(k, ALICE);
        kid
    };

    // Bob locks their object as well.
    let ik2 = {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
        kid
    };

    // Alice gives the custodian their object to hold in escrow.
    {
        ts.next_tx(ALICE);
        let k1: Key = ts.take_from_sender();
        let l1: Locked<Coin<SUI>> = ts.take_from_sender();
        create(k1, l1, ik2, BOB, CUSTODIAN, ts.ctx());
    };

    // Bob has a change of heart, so they unlock the object and tamper
    // with it.
    {
        ts.next_tx(BOB);
        let k: Key = ts.take_from_sender();
        let l: Locked<Coin<SUI>> = ts.take_from_sender();
        let mut c = lock::unlock(l, k);

        let _dust = coin::split(&mut c, 1, ts.ctx());
        let (l, k) = lock::lock(c, ts.ctx());
        create(k, l, ik1, ALICE, CUSTODIAN, ts.ctx());
    };

    // When the Custodian makes the swap, it detects Bob's nefarious
    // behaviour.
    {
        ts.next_tx(CUSTODIAN);
        swap<Coin<SUI>, Coin<SUI>>(
            ts.take_from_sender(),
            ts.take_from_sender(),
        );
    };

    abort 1337
}

#[test]
fun test_return_to_sender() {
    let mut ts = ts::begin(@0x0);

    // Alice locks the object they want to trade
    let cid = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        let (l, k) = lock::lock(c, ts.ctx());
        let i = object::id_from_address(@0x0);
        create(k, l, i, BOB, CUSTODIAN, ts.ctx());
        cid
    };

    // Custodian sends it back
    {
        ts.next_tx(CUSTODIAN);
        return_to_sender<Coin<SUI>>(ts.take_from_sender());
    };

    ts.next_tx(@0x0);

    // Alice can then access it.
    {
        let c: Coin<SUI> = ts.take_from_address_by_id(ALICE, cid);
        ts::return_to_address(ALICE, c)
    };

    ts.end();
}
