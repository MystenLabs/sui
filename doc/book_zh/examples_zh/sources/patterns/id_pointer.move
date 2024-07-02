// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 在这个例子中将实现一个简单的`Lock`和`Key`的机制（锁和钥匙）
/// `Lock<T>`是一个共享对象可以包含任意对象，`Key`是一个被拥有的对象，
/// 需要`Key`才能访问`Lock`的内容
/// This example implements a simple `Lock` and `Key` mechanics
/// on Sui where `Lock<T>` is a shared object that can contain any object,
/// and `Key` is an owned object which is required to get access to the
/// contents of the lock.
///
/// `Key`通过`ID`字段与`Lock`相关联。这样的检查允许链下发现目标，同时将动态可转让的功能与“静态”内容分离。
/// 另一个好处是目标资产始终可以被发现，而其`Key`可以被包装到另一个对象中（例如，市场列表）。
/// `Key` is linked to its `Lock` using an `ID` field. This check allows
/// off-chain discovery of the target as well as splits the dynamic
/// transferable capability and the 'static' contents. Another benefit of
/// this approach is that the target asset is always discoverable while its
/// `Key` can be wrapped into another object (eg a marketplace listing).
module examples::lock_and_key {
    use sui::object::{Self, ID, UID};
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};
    use std::option::{Self, Option};

    /// 错误码 0： `Lock`为空
    /// Lock is empty, nothing to take.
    const ELockIsEmpty: u64 = 0;

    /// 错误码 1： `Lock`与`Key`不匹配
    /// Key does not match the Lock.
    const EKeyMismatch: u64 = 1;

    /// 错误码 2： `Lock`已被使用
    /// Lock already contains something.
    const ELockIsFull: u64 = 2;

    /// `Lock`容器可以存放任意内容
    /// Lock that stores any content inside it.
    struct Lock<T: store + key> has key {
        id: UID,
        locked: Option<T>
    }

    /// `Key`对象伴随`Lock`一同生成，它是可以变更所有权的，同时可以打开`Lock`
    /// A key that is created with a Lock; is transferable
    /// and contains all the needed information to open the Lock.
    struct Key<phantom T: store + key> has key, store {
        id: UID,
        for: ID,
    }

    /// 返回`Key`对象所对应的`Lock`对象的ID
    /// Returns an ID of a Lock for a given Key.
    public fun key_for<T: store + key>(key: &Key<T>): ID {
        key.for
    }

    /// 在`Lock`中保存一些内容并设置为共享对象。生成对应的`Key`对象。
    /// 例如我们可以利用`Lock`保存一些SUI代币
    /// Lock some content inside a shared object. A Key is created and is
    /// sent to the transaction sender. For example, we could turn the
    /// lock into a treasure chest by locking some `Coin<SUI>` inside.
    ///
    /// 交易发送者获得`Key`
    /// Sender gets the `Key` to this `Lock`.
    public entry fun create<T: store + key>(obj: T, ctx: &mut TxContext) {
        let id = object::new(ctx);
        let for = object::uid_to_inner(&id);

        transfer::share_object(Lock<T> {
            id,
            locked: option::some(obj),
        });

        transfer::transfer(Key<T> {
            for,
            id: object::new(ctx)
        }, tx_context::sender(ctx));
    }

    /// 将某种对象锁在`Lock`中，当`Key`不匹配或者`Lock`中已经保存了内容时报错。
    /// Lock something inside a shared object using a Key. Aborts if
    /// lock is not empty or if key doesn't match the lock.
    public entry fun lock<T: store + key>(
        obj: T,
        lock: &mut Lock<T>,
        key: &Key<T>,
    ) {
        assert!(option::is_none(&lock.locked), ELockIsFull);
        assert!(&key.for == object::borrow_id(lock), EKeyMismatch);

        option::fill(&mut lock.locked, obj);
    }

    /// 利用`Key`解锁`Lock`并获得保存的对象。
    /// 当`Key`不匹配或者`Lock`中无内容时报错。
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

    /// 利用`Key`解锁`Lock`并获得保存的对象, 将保存的对象转移给交易发起者
    /// Unlock the Lock and transfer its contents to the transaction sender.
    public fun take<T: store + key>(
        lock: &mut Lock<T>,
        key: &Key<T>,
        ctx: &mut TxContext,
    ) {
        transfer::public_transfer(unlock(lock, key), tx_context::sender(ctx))
    }
}
