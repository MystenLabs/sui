// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::item {
    use sui::transfer;
    use sui::object::{Self, UID};
    use std::string::{Self, String};
    use sui::tx_context::{Self, TxContext};

    /// `AdminCap`类型与创建新`Item`对象的权限绑定
    /// Type that marks Capability to create new `Item`s.
    struct AdminCap has key { id: UID }

    /// 自定义的类似NFT的类型
    /// Custom NFT-like type.
    struct Item has key, store { id: UID, name: String }

    /// 模块的初始化函数只在模块初始化时调用一次，
    /// 因此只会存在一个`AdminCap`，并且这个权限凭证转移给了模块发布者
    /// Module initializer is called once on module publish.
    /// Here we create only one instance of `AdminCap` and send it to the publisher.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(AdminCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// 这个入口函数只能被拥有`AdminCap`对象的发送者调用
    /// The entry function can not be called if `AdminCap` is not passed as
    /// the first argument. Hence only owner of the `AdminCap` can perform
    /// this action.
    public entry fun create_and_send(
        _: &AdminCap, name: vector<u8>, to: address, ctx: &mut TxContext
    ) {
        transfer::transfer(Item {
            id: object::new(ctx),
            name: string::utf8(name)
        }, to)
    }
}
