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
module escrow::shared;

use escrow::lock::{Locked, Key};
use sui::{dynamic_object_field as dof, event};

/// The `name` of the DOF that holds the Escrowed object.
/// Allows easy discoverability for the escrowed object.
public struct EscrowedObjectKey has copy, store, drop {}

/// An object held in escrow
///
/// The escrowed object is added as a Dynamic Object Field so it can still be looked-up.
public struct Escrow<phantom T: key + store> has key, store {
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

//docs::#noemit
public fun create<T: key + store>(
    escrowed: T,
    exchange_key: ID,
    recipient: address,
    ctx: &mut TxContext,
) {
    let mut escrow = Escrow<T> {
        id: object::new(ctx),
        sender: ctx.sender(),
        recipient,
        exchange_key,
    };

    //docs::#noemit-pause
    event::emit(EscrowCreated {
        escrow_id: object::id(&escrow),
        key_id: exchange_key,
        sender: escrow.sender,
        recipient,
        item_id: object::id(&escrowed),
    });
    //docs::#noemit-resume

    dof::add(&mut escrow.id, EscrowedObjectKey {}, escrowed);

    transfer::public_share_object(escrow);
}
//docs::/#noemit

/// The `recipient` of the escrow can exchange `obj` with the escrowed item
public fun swap<T: key + store, U: key + store>(
    mut escrow: Escrow<T>,
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

    assert!(recipient == ctx.sender(), EMismatchedSenderRecipient);
    assert!(exchange_key == object::id(&key), EMismatchedExchangeObject);

    // Do the actual swap
    transfer::public_transfer(locked.unlock(key), sender);

    event::emit(EscrowSwapped {
        escrow_id: id.to_inner(),
    });

    id.delete();

    escrowed
}

/// The `creator` can cancel the escrow and get back the escrowed item
public fun return_to_sender<T: key + store>(mut escrow: Escrow<T>, ctx: &TxContext): T {
    event::emit(EscrowCancelled {
        escrow_id: object::id(&escrow),
    });

    let escrowed = dof::remove<EscrowedObjectKey, T>(&mut escrow.id, EscrowedObjectKey {});

    let Escrow {
        id,
        sender,
        recipient: _,
        exchange_key: _,
    } = escrow;

    assert!(sender == ctx.sender(), EMismatchedSenderRecipient);
    id.delete();
    escrowed
}

// === Events ===
public struct EscrowCreated has copy, drop {
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

public struct EscrowSwapped has copy, drop {
    escrow_id: ID,
}

public struct EscrowCancelled has copy, drop {
    escrow_id: ID,
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
const DIANE: address = @0xD;

#[test_only]
fun test_coin(ts: &mut Scenario): Coin<SUI> {
    coin::mint_for_testing<SUI>(42, ts.ctx())
}

//docs::#test
#[test]
fun test_successful_swap() {
    let mut ts = ts::begin(@0x0);

    //docs::#test-pause:// Rest of the test ...

    // Bob locks the object they want to trade.
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

    // Alice creates a public Escrow holding the object they are willing to
    // share, and the object they want from Bob
    let i1 = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        create(c, ik2, BOB, ts.ctx());
        cid
    };

    // Bob responds by offering their object, and gets Alice's object in
    // return.
    // docs::#bob
    {
        ts.next_tx(BOB);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        let c = escrow.swap(k2, l2, ts.ctx());

        transfer::public_transfer(c, BOB);
    };
    // docs::/#bob

    // docs::#finish
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
    // docs::/#finish
    //docs::#test-resume

    ts::end(ts);
}
//docs::/#test

#[test]
#[expected_failure(abort_code = EMismatchedSenderRecipient)]
fun test_mismatch_sender() {
    let mut ts = ts::begin(@0x0);

    let ik2 = {
        ts.next_tx(DIANE);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, DIANE);
        transfer::public_transfer(k, DIANE);
        kid
    };

    // Alice wants to trade with Bob.
    {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        create(c, ik2, BOB, ts.ctx());
    };

    // But Diane is the one who attempts the swap
    {
        ts.next_tx(DIANE);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        let c = escrow.swap(k2, l2, ts.ctx());

        transfer::public_transfer(c, DIANE);
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EMismatchedExchangeObject)]
fun test_mismatch_object() {
    let mut ts = ts::begin(@0x0);

    {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
    };

    // Alice wants to trade with Bob, but Alice has asked for an object (via
    // its `exchange_key`) that Bob has not put up for the swap.
    {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        create(c, cid, BOB, ts.ctx());
    };

    // When Bob tries to complete the swap, it will fail, because they
    // cannot meet Alice's requirements.
    {
        ts.next_tx(BOB);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        let c = escrow.swap(k2, l2, ts.ctx());

        transfer::public_transfer(c, BOB);
    };

    abort 1337
}

#[test]
#[expected_failure(abort_code = EMismatchedExchangeObject)]
fun test_object_tamper() {
    let mut ts = ts::begin(@0x0);

    // Bob locks their object.
    let ik2 = {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
        kid
    };

    // Alice sets up the escrow
    {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        create(c, ik2, BOB, ts.ctx());
    };

    // Bob has a change of heart, so they unlock the object and tamper with
    // it before initiating the swap, but it won't be possible for Bob to
    // hide their tampering.
    {
        ts.next_tx(BOB);
        let k: Key = ts.take_from_sender();
        let l: Locked<Coin<SUI>> = ts.take_from_sender();
        let mut c = lock::unlock(l, k);

        let _dust = c.split(1, ts.ctx());
        let (l, k) = lock::lock(c, ts.ctx());
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let c = escrow.swap(k, l, ts.ctx());

        transfer::public_transfer(c, BOB);
    };

    abort 1337
}

#[test]
fun test_return_to_sender() {
    let mut ts = ts::begin(@0x0);

    // Alice puts up the object they want to trade
    let cid = {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        let cid = object::id(&c);
        let i = object::id_from_address(@0x0);
        create(c, i, BOB, ts.ctx());
        cid
    };

    // ...but has a change of heart and takes it back
    {
        ts.next_tx(ALICE);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let c = escrow.return_to_sender(ts.ctx());

        transfer::public_transfer(c, ALICE);
    };

    ts.next_tx(@0x0);

    // Alice can then access it.
    {
        let c: Coin<SUI> = ts.take_from_address_by_id(ALICE, cid);
        ts::return_to_address(ALICE, c)
    };

    ts::end(ts);
}

#[test]
#[expected_failure]
fun test_return_to_sender_failed_swap() {
    let mut ts = ts::begin(@0x0);

    // Bob locks their object.
    let ik2 = {
        ts.next_tx(BOB);
        let c = test_coin(&mut ts);
        let (l, k) = lock::lock(c, ts.ctx());
        let kid = object::id(&k);
        transfer::public_transfer(l, BOB);
        transfer::public_transfer(k, BOB);
        kid
    };

    // Alice creates a public Escrow holding the object they are willing to
    // share, and the object they want from Bob
    {
        ts.next_tx(ALICE);
        let c = test_coin(&mut ts);
        create(c, ik2, BOB, ts.ctx());
    };

    // ...but then has a change of heart
    {
        ts.next_tx(ALICE);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let c = escrow.return_to_sender(ts.ctx());
        transfer::public_transfer(c, ALICE);
    };

    // Bob's attempt to complete the swap will now fail.
    {
        ts.next_tx(BOB);
        let escrow: Escrow<Coin<SUI>> = ts.take_shared();
        let k2: Key = ts.take_from_sender();
        let l2: Locked<Coin<SUI>> = ts.take_from_sender();
        let c = escrow.swap(k2, l2, ts.ctx());

        transfer::public_transfer(c, BOB);
    };

    abort 1337
}
