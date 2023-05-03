// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 一个用于任意转移的封装模块
/// A freely transfererrable Wrapper for custom data.
module examples::wrapper {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;

    /// 当一个对象具有`store`属性时可以在任何模块中进行转移，同时
    /// 不需要自定义的转移函数
    /// An object with `store` can be transferred in any
    /// module without a custom transfer implementation.
    struct Wrapper<T: store> has key, store {
        id: UID,
        contents: T
    }

    /// 视图函数，用于获取`content`的内容
    /// View function to read contents of a `Container`.
    public fun contents<T: store>(c: &Wrapper<T>): &T {
        &c.contents
    }

    /// 任何人都可以创建一个新的对象
    /// Anyone can create a new object
    public fun create<T: store>(
        contents: T, ctx: &mut TxContext
    ): Wrapper<T> {
        Wrapper {
            contents,
            id: object::new(ctx),
        }
    }

    /// 摧毁封装对象，获取其封装的`contents`
    /// Destroy `Wrapper` and get T.
    public fun destroy<T: store> (c: Wrapper<T>): T {
        let Wrapper { id, contents } = c;
        object::delete(id);
        contents
    }
}

module examples::profile {
    use sui::transfer;
    use sui::url::{Self, Url};
    use std::string::{Self, String};
    use sui::tx_context::{Self, TxContext};

    // 声明对封装模块的引用
    // using Wrapper functionality
    use 0x0::wrapper;

    /// 档案信息不是一个对象，可以被包括在可被转移的容器中
    /// Profile information, not an object, can be wrapped
    /// into a transferable container
    struct ProfileInfo has store {
        name: String,
        url: Url
    }

    /// 读取`ProfileInfo`中的`name`项
    /// Read `name` field from `ProfileInfo`.
    public fun name(info: &ProfileInfo): &String {
        &info.name
    }

    /// 读取`ProfileInfo`中的`url`项
    /// Read `url` field from `ProfileInfo`.
    public fun url(info: &ProfileInfo): &Url {
        &info.url
    }

    /// 创建新的`ProfileInfo`同时封装在`Wrapper`对象中
    /// 将创建的对象转移给交易的发起者
    /// Creates new `ProfileInfo` and wraps into `Wrapper`.
    /// Then transfers to sender.
    public fun create_profile(
        name: vector<u8>, url: vector<u8>, ctx: &mut TxContext
    ) {
        // create a new container and wrap ProfileInfo into it
        let container = wrapper::create(ProfileInfo {
            name: string::utf8(name),
            url: url::new_unsafe_from_bytes(url)
        }, ctx);

        // `Wrapper` type is freely transferable
        transfer::public_transfer(container, tx_context::sender(ctx))
    }
}
