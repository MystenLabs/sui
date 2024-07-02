// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 在这个模块中定义了一个泛型类型`Guardian<T>`，并且只可使用witness创建
/// Module that defines a generic type `Guardian<T>` which can only be
/// instantiated with a witness.
module examples::guardian {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// 虚参数`T`只可被`create_guardian`初始化， 同时这个类型需要具备`drop`修饰符
    /// Phantom parameter T can only be initialized in the `create_guardian`
    /// function. But the types passed here must have `drop`.
    struct Guardian<phantom T: drop> has key, store {
        id: UID
    }

    /// 第一个参数是一个类型T具有`drop`能力的示例，它会在接受后被丢弃
    /// The first argument of this function is an actual instance of the
    /// type T with `drop` ability. It is dropped as soon as received.
    public fun create_guardian<T: drop>(
        _witness: T, ctx: &mut TxContext
    ): Guardian<T> {
        Guardian { id: object::new(ctx) }
    }
}

/// 一个自定义的模块，用来使用`guardian`模块
/// Custom module that makes use of the `guardian`.
module examples::peace_guardian {
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    // 引入`guardian`依赖
    // Use the `guardian` as a dependency.
    use 0x0::guardian;

    /// `PEACE`类型只被使用一次
    /// This type is intended to be used only once.
    struct PEACE has drop {}

    /// 模块初始化函数可以确保其中的代码只被执行一次，这也是见证人模式最常使用方法
    /// Module initializer is the best way to ensure that the
    /// code is called only once. With `Witness` pattern it is
    /// often the best practice.
    fun init(ctx: &mut TxContext) {
        transfer::public_transfer(
            guardian::create_guardian(PEACE {}, ctx),
            tx_context::sender(ctx)
        )
    }
}
