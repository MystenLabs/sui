// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::mycoin {
    use std::option;
    use sui::coin;
    use sui::transfer;
    use sui::tx_context::{Self, TxContext};

    /// 这个结构体代表代币类型，每种代币都需要一个类型：`Coin<package_object::mycoin::MYCOIN>`
    /// 确保结构体名与模块名匹配
    /// The type identifier of coin. The coin will have a type
    /// tag of kind: `Coin<package_object::mycoin::MYCOIN>`
    /// Make sure that the name of the type matches the module's name.
    struct MYCOIN has drop {}

    /// 模块初始化函数在模块发布时被调用。
    /// `TreasuryCap`会被发送给模块的发布者，因此发布者可以控制代币铸造和销毁。
    /// Module initializer is called once on module publish. A treasury
    /// cap is sent to the publisher, who then controls minting and burning
    fun init(witness: MYCOIN, ctx: &mut TxContext) {
        let (treasury, metadata) = coin::create_currency(witness, 6, b"MYCOIN", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury, tx_context::sender(ctx))
    }
}
