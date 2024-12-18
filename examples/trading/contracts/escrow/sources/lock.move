// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// The `lock` module offers an API for wrapping any object that has
/// `store` and protecting it with a single-use `Key`.
///
/// This is used to commit to swapping a particular object in a
/// particular, fixed state during escrow.
module escrow::lock;

use sui::{dynamic_object_field as dof, event};

/// The `name` of the DOF that holds the Locked object.
/// Allows better discoverability for the locked object.
public struct LockedObjectKey has copy, store, drop {}

/// A wrapper that protects access to `obj` by requiring access to a `Key`.
///
/// Used to ensure an object is not modified if it might be involved in a
/// swap.
///
/// Object is added as a Dynamic Object Field so that it can still be looked-up.
public struct Locked<phantom T: key + store> has key, store {
    id: UID,
    key: ID,
}

/// Key to open a locked object (consuming the `Key`)
public struct Key has key, store { id: UID }

// === Error codes ===

/// The key does not match this lock.
const ELockKeyMismatch: u64 = 0;

// === Public Functions ===

/// Lock `obj` and get a key that can be used to unlock it.
public fun lock<T: key + store>(obj: T, ctx: &mut TxContext): (Locked<T>, Key) {
    let key = Key { id: object::new(ctx) };
    let mut lock = Locked {
        id: object::new(ctx),
        key: object::id(&key),
    };

    event::emit(LockCreated {
        lock_id: object::id(&lock),
        key_id: object::id(&key),
        creator: ctx.sender(),
        item_id: object::id(&obj),
    });

    // Adds the `object` as a DOF for the `lock` object
    dof::add(&mut lock.id, LockedObjectKey {}, obj);

    (lock, key)
}

/// Unlock the object in `locked`, consuming the `key`.  Fails if the wrong
/// `key` is passed in for the locked object.
public fun unlock<T: key + store>(mut locked: Locked<T>, key: Key): T {
    assert!(locked.key == object::id(&key), ELockKeyMismatch);
    let Key { id } = key;
    id.delete();

    let obj = dof::remove<LockedObjectKey, T>(&mut locked.id, LockedObjectKey {});

    event::emit(LockDestroyed { lock_id: object::id(&locked) });

    let Locked { id, key: _ } = locked;
    id.delete();
    obj
}

// === Events ===
public struct LockCreated has copy, drop {
    /// The ID of the `Locked` object.
    lock_id: ID,
    /// The ID of the key that unlocks a locked object in a `Locked`.
    key_id: ID,
    /// The creator of the locked object.
    creator: address,
    /// The ID of the item that is locked.
    item_id: ID,
}

public struct LockDestroyed has copy, drop {
    /// The ID of the `Locked` object.
    lock_id: ID,
}

// === Tests ===
#[test_only]
use sui::coin::{Self, Coin};
#[test_only]
use sui::sui::SUI;
#[test_only]
use sui::test_scenario::{Self as ts, Scenario};

#[test_only]
fun test_coin(ts: &mut Scenario): Coin<SUI> {
    coin::mint_for_testing<SUI>(42, ts.ctx())
}

#[test]
fun test_lock_unlock() {
    let mut ts = ts::begin(@0xA);
    let coin = test_coin(&mut ts);

    let (lock, key) = lock(coin, ts.ctx());
    let coin = lock.unlock(key);

    coin.burn_for_testing();
    ts.end();
}

#[test]
#[expected_failure(abort_code = ELockKeyMismatch)]
fun test_lock_key_mismatch() {
    let mut ts = ts::begin(@0xA);
    let coin = test_coin(&mut ts);
    let another_coin = test_coin(&mut ts);
    let (l, _k) = lock(coin, ts.ctx());
    let (_l, k) = lock(another_coin, ts.ctx());

    let _key = l.unlock(k);
    abort 1337
}
