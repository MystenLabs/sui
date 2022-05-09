// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// An example of a module that uses Shared Objects and ID linking/access.
///
/// This module allows any content to be locked inside a 'virtual chest' and later
/// be accessed by putting a 'key' into the 'lock'. Lock is shared and is visible
/// and discoverable by the key owner.
module Basics::Lock {
    use Sui::ID::{Self, ID, VersionedID};
    use Sui::Transfer;
    use Sui::TxContext::{Self, TxContext};
    use Std::Option::{Self, Option};

    /// Lock is empty, nothing to take.
    const ELockIsEmpty: u64 = 0;

    /// Key does not match the Lock.
    const EKeyMismatch: u64 = 1;

    /// Lock already contains something.
    const ELockIsFull: u64 = 2;

    /// Lock that stores any content inside it.
    struct Lock<T: store + key> has key, store {
        id: VersionedID,
        locked: Option<T>
    }

    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock.
    struct Key<phantom T: store + key> has key, store {
        id: VersionedID,
        for: ID,
    }

    /// Returns an ID of a Lock for a given Key.
    public fun key_for<T: store + key>(key: &Key<T>): ID {
        key.for
    }

    /// Lock some content inside a shared object. A Key is created and is
    /// sent to the transaction sender.
    public(script) fun create<T: store + key>(obj: T, ctx: &mut TxContext) {
        let id = TxContext::new_id(ctx);
        let for = *ID::inner(&id);

        Transfer::share_object(Lock<T> {
            id,
            locked: Option::some(obj),
        });

        Transfer::transfer(Key<T> {
            for,
            id: TxContext::new_id(ctx)
        }, TxContext::sender(ctx));
    }

    /// Lock something inside a shared object using a Key. Aborts if
    /// lock is not empty or if key doesn't match the lock.
    public(script) fun lock<T: store + key>(
        obj: T,
        lock: &mut Lock<T>,
        key: &Key<T>,
        _ctx: &mut TxContext,
    ) {
        assert!(Option::is_none(&lock.locked), ELockIsFull);
        assert!(&key.for == ID::id(lock), EKeyMismatch);

        Option::fill(&mut lock.locked, obj);
    }

    /// Unlock the Lock with a Key and access its contents.
    /// Can only be called if both conditions are met:
    /// - key matches the lock
    /// - lock is not empty
    public fun unlock<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
    ): T {
        assert!(Option::is_some(&lock.locked), ELockIsEmpty);
        assert!(&key.for == ID::id(lock), EKeyMismatch);

        Option::extract(&mut lock.locked)
    }

    /// Unlock the Lock and transfer its contents to the transaction sender.
    public fun take<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
        ctx: &mut TxContext,
    ) {
        Transfer::transfer(unlock(lock, key), TxContext::sender(ctx))
    }
}

#[test_only]
module Basics::LockTest {
    use Sui::ID::VersionedID;
    use Sui::TestScenario;
    use Sui::TxContext;
    use Sui::Transfer;
    use Basics::Lock::{Self, Lock, Key};

    /// Custom structure which we will store inside a Lock.
    struct Treasure has store, key {
        id: VersionedID
    }

    #[test]
    public(script) fun test_lock() {
        let user1 = @0x1;
        let user2 = @0x2;

        let scenario = &mut TestScenario::begin(&user1);

        // User1 creates a lock and places his treasure inside.
        TestScenario::next_tx(scenario, &user1);
        {
            let ctx = TestScenario::ctx(scenario);
            let id = TxContext::new_id(ctx);

            Lock::create(Treasure { id }, ctx);
        };

        // Now User1 owns a key from the lock. He decides to send this
        // key to User2, so that he can have access to the stored treasure.
        TestScenario::next_tx(scenario, &user1);
        {
            let key = TestScenario::take_owned<Key<Treasure>>(scenario);

            Transfer::transfer(key, user2);
        };

        // User2 is impatient and he decides to take the treasure.
        TestScenario::next_tx(scenario, &user2);
        {
            let lock_wrapper = TestScenario::take_shared<Lock<Treasure>>(scenario);
            let lock = TestScenario::borrow_mut(&mut lock_wrapper);
            let key = TestScenario::take_owned<Key<Treasure>>(scenario);
            let ctx = TestScenario::ctx(scenario);

            Lock::take<Treasure>(lock, &key, ctx);

            TestScenario::return_shared(scenario, lock_wrapper);
            TestScenario::return_owned(scenario, key);
        };
    }
}
