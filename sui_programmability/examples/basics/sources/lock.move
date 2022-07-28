// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a module that uses Shared Objects and ID linking/access.
///
/// This module allows any content to be locked inside a 'virtual chest' and later
/// be accessed by putting a 'key' into the 'lock'. Lock is shared and is visible
/// and discoverable by the key owner.
module basics::lock {
    use sui::object::{Self, ID, Info};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::option::{Self, Option};

    /// Lock is empty, nothing to take.
    const ELockIsEmpty: u64 = 0;

    /// Key does not match the Lock.
    const EKeyMismatch: u64 = 1;

    /// Lock already contains something.
    const ELockIsFull: u64 = 2;

    /// Lock that stores any content inside it.
    struct Lock<T: store + key> has key, store {
        info: Info,
        locked: Option<T>
    }

    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock.
    struct Key<phantom T: store + key> has key, store {
        info: Info,
        for: ID,
    }

    /// Returns an ID of a Lock for a given Key.
    public fun key_for<T: store + key>(key: &Key<T>): ID {
        key.for
    }

    /// Lock some content inside a shared object. A Key is created and is
    /// sent to the transaction sender.
    public entry fun create<T: store + key>(obj: T, ctx: &mut TxContext) {
        let info = object::new(ctx);
        let for = *object::info_id(&info);

        transfer::share_object(Lock<T> {
            info,
            locked: option::some(obj),
        });

        transfer::transfer(Key<T> {
            for,
            info: object::new(ctx)
        }, tx_context::sender(ctx));
    }

    /// Lock something inside a shared object using a Key. Aborts if
    /// lock is not empty or if key doesn't match the lock.
    public entry fun lock<T: store + key>(
        obj: T,
        lock: &mut Lock<T>,
        key: &Key<T>,
    ) {
        assert!(option::is_none(&lock.locked), ELockIsFull);
        assert!(&key.for == object::id(lock), EKeyMismatch);

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
        assert!(&key.for == object::id(lock), EKeyMismatch);

        option::extract(&mut lock.locked)
    }

    /// Unlock the Lock and transfer its contents to the transaction sender.
    public fun take<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
        ctx: &mut TxContext,
    ) {
        transfer::transfer(unlock(lock, key), tx_context::sender(ctx))
    }
}

#[test_only]
module basics::lockTest {
    use sui::object::{Self, Info};
    use sui::test_scenario;
    use sui::transfer;
    use basics::lock::{Self, Lock, Key};

    /// Custom structure which we will store inside a Lock.
    struct Treasure has store, key {
        info: Info
    }

    #[test]
    fun test_lock() {
        let user1 = @0x1;
        let user2 = @0x2;

        let scenario = &mut test_scenario::begin(&user1);

        // User1 creates a lock and places his treasure inside.
        test_scenario::next_tx(scenario, &user1);
        {
            let ctx = test_scenario::ctx(scenario);
            let info = object::new(ctx);

            lock::create(Treasure { info }, ctx);
        };

        // Now User1 owns a key from the lock. He decides to send this
        // key to User2, so that he can have access to the stored treasure.
        test_scenario::next_tx(scenario, &user1);
        {
            let key = test_scenario::take_owned<Key<Treasure>>(scenario);

            transfer::transfer(key, user2);
        };

        // User2 is impatient and he decides to take the treasure.
        test_scenario::next_tx(scenario, &user2);
        {
            let lock_wrapper = test_scenario::take_shared<Lock<Treasure>>(scenario);
            let lock = test_scenario::borrow_mut(&mut lock_wrapper);
            let key = test_scenario::take_owned<Key<Treasure>>(scenario);
            let ctx = test_scenario::ctx(scenario);

            lock::take<Treasure>(lock, &key, ctx);

            test_scenario::return_shared(scenario, lock_wrapper);
            test_scenario::return_owned(scenario, key);
        };
    }
}
