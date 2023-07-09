// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a module that uses Shared Objects and ID linking/access.
///
/// This module allows any content to be locked inside a 'virtual chest' and later
/// be accessed by putting a 'key' into the 'lock'. Lock is shared and is visible
/// and discoverable by the key owner.
module basics::lock {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::TxContext;
    use std::option::{Self, Option};

    /// Lock is empty, nothing to take.
    const ELockIsEmpty: u64 = 0;

    /// Key does not match the Lock.
    const EKeyMismatch: u64 = 1;

    /// Lock already contains something.
    const ELockIsFull: u64 = 2;

    /// Lock that stores any content inside it.
    struct Lock<T: store + key> has key, store {
        id: UID,
        locked: Option<T>
    }

    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock.
    struct Key<phantom T: store + key> has key, store {
        id: UID,
        for: ID,
    }

    /// Returns an ID of a Lock for a given Key.
    public fun key_for<T: store + key>(key: &Key<T>): ID {
        key.for
    }

    /// Lock some content inside a shared object. A Key is created and is
    /// sent to the transaction sender.
    public fun create<T: store + key>(obj: T, ctx: &mut TxContext): Key<T> {
        let id = object::new(ctx);
        let for = object::uid_to_inner(&id);

        transfer::public_share_object(Lock<T> {
            id,
            locked: option::some(obj),
        });

        Key<T> {
            for,
            id: object::new(ctx)
        }
    }

    /// Lock something inside a shared object using a Key. Aborts if
    /// lock is not empty or if key doesn't match the lock.
    public fun lock<T: store + key>(
        obj: T,
        lock: &mut Lock<T>,
        key: &Key<T>,
    ) {
        assert!(option::is_none(&lock.locked), ELockIsFull);
        assert!(&key.for == object::borrow_id(lock), EKeyMismatch);

        option::fill(&mut lock.locked, obj);
    }

    /// Unlock the Lock with a Key and access its contents.
    /// Can only be called if both conditions are met:
    /// - key matches the lock
    /// - lock is not empty
    public fun unlock<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
    ): T {
        assert!(option::is_some(&lock.locked), ELockIsEmpty);
        assert!(&key.for == object::borrow_id(lock), EKeyMismatch);

        option::extract(&mut lock.locked)
    }
}

#[test_only]
module basics::lockTest {
    use sui::object::{Self, UID};
    use sui::test_scenario;
    use sui::transfer;
    use sui::tx_context;
    use basics::lock::{Self, Lock, Key};

    /// Custom structure which we will store inside a Lock.
    struct Treasure has store, key {
        id: UID
    }

    #[test]
    fun test_lock() {
        let user1 = @0x1;
        let user2 = @0x2;

        let scenario_val = test_scenario::begin(user1);
        let scenario = &mut scenario_val;

        // User1 creates a lock and places his treasure inside.
        test_scenario::next_tx(scenario, user1);
        {
            let ctx = test_scenario::ctx(scenario);
            let id = object::new(ctx);

            let l = lock::create(Treasure { id }, ctx);
            transfer::public_transfer(l, tx_context::sender(ctx))
        };

        // Now User1 owns a key from the lock. He decides to send this
        // key to User2, so that he can have access to the stored treasure.
        test_scenario::next_tx(scenario, user1);
        {
            let key = test_scenario::take_from_sender<Key<Treasure>>(scenario);
            transfer::public_transfer(key, user2);
        };

        // User2 is impatient and he decides to take the treasure.
        test_scenario::next_tx(scenario, user2);
        {
            let lock_val = test_scenario::take_shared<Lock<Treasure>>(scenario);
            let lock = &mut lock_val;
            let key = test_scenario::take_from_sender<Key<Treasure>>(scenario);
            let ctx = test_scenario::ctx(scenario);

            let l = lock::unlock<Treasure>(lock, &key);
            transfer::public_transfer(l, tx_context::sender(ctx));

            test_scenario::return_shared(lock_val);
            test_scenario::return_to_sender(scenario, key);
        };
        test_scenario::end(scenario_val);
    }
}
