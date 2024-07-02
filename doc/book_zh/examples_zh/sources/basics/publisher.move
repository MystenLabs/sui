// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 在这个模块中，将定义一个OTW，并为模块发布者创建/声明一个`Publisher`对象
/// A simple package that defines an OTW and claims a `Publisher`
/// object for the sender.
module examples::owner {
    use sui::tx_context::TxContext;
    use sui::package;

    /// OTW 是一个与模块名相同（字母大写）并只含有`drop`的结构体
    /// OTW is a struct with only `drop` and is named
    /// after the module - but uppercased. See "One Time
    /// Witness" page for more details.
    struct OWNER has drop {}

    /// 定义另一个类型
    /// Some other type to use in a dummy check
    struct ThisType {}

    /// 在模块发布后，交易的发起者将获得一个`Publisher`对象。
    /// 可以用来在`Kiosk`系统中设置对象的显示或管理对象转移的规则。
    /// After the module is published, the sender will receive
    /// a `Publisher` object. Which can be used to set Display
    /// or manage the transfer policies in the `Kiosk` system.
    fun init(otw: OWNER, ctx: &mut TxContext) {
        package::claim_and_keep(otw, ctx)
    }
}

/// 一个利用“Publisher”对象为其所拥有的类型创建“TypeOwnerCap”对象的示例。
/// A module that utilizes the `Publisher` object to give a token
/// of appreciation and a `TypeOwnerCap` for the owned type.
module examples::type_owner {
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::package::{Self, Publisher};

    /// 错误码0：当提供的类型无法与给定的`Publisher`匹配时触发
    /// Trying to claim ownership of a type with a wrong `Publisher`.
    const ENotOwner: u64 = 0;

    /// 当所给的类型（`T`）可以用给定的`Publisher`证明来源时，所创建的权限凭证
    /// A capability granted to those who want an "objective"
    /// confirmation of their ownership :)
    struct TypeOwnerCap<phantom T> has key, store {
        id: UID
    }

    /// 利用`Publisher`检查调用者是否拥有类型`T`
    /// Uses the `Publisher` object to check if the caller owns the type `T`.
    public fun prove_ownership<T>(
        publisher: &Publisher, ctx: &mut TxContext
    ): TypeOwnerCap<T> {
        assert!(package::from_package<T>(publisher), ENotOwner);
        TypeOwnerCap<T> { id: object::new(ctx) }
    }
}
