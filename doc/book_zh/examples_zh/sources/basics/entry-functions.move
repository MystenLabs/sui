// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::object {
    use sui::transfer;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    struct Object has key {
        id: UID
    }

    /// 被`public`修饰的的函数可以被任意模块调用
    /// 被`entry`修饰的函数不可以返回值
    /// If function is defined as public - any module can call it.
    /// Non-entry functions are also allowed to have return values.
    public fun create(ctx: &mut TxContext): Object {
        Object { id: object::new(ctx) }
    }

    /// 入口函数不可以拥有返回值因为它们可以在交易中被直接调用，返回值也不可用
    /// 如果入口`entry`函数如果没有同时被`public`修饰将不可以被其他模块调用
    /// Entrypoints can't have return values as they can only be called
    /// directly in a transaction and the returned value can't be used.
    /// However, `entry` without `public` disallows calling this method from
    /// other Move modules.
    entry fun create_and_transfer(to: address, ctx: &mut TxContext) {
        transfer::transfer(create(ctx), to)
    }
}
