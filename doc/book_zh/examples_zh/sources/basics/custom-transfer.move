// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::restricted_transfer {
    use sui::tx_context::{Self, TxContext};
    use sui::balance::{Self, Balance};
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::transfer;
    use sui::sui::SUI;
    
    /// 错误码 0： 对应所付费用与转移费用不符
    /// For when paid amount is not equal to the transfer price.
    const EWrongAmount: u64 = 0;

    /// 可以产生新`TitleDeed`的权限凭证
    /// A Capability that allows bearer to create new `TitleDeed`s.
    struct GovernmentCapability has key { id: UID }

    /// 用于表示产权的凭证，只能有具备`GovernmentCapability`权限的账户产生
    /// An object that marks a property ownership. Can only be issued
    /// by an authority.
    struct TitleDeed has key {
        id: UID,
        // ... some additional fields
    }

    /// 一个中心化的财产所有权变更和收费的登记处
    /// A centralized registry that approves property ownership
    /// transfers and collects fees.
    struct LandRegistry has key {
        id: UID,
        balance: Balance<SUI>,
        fee: u64
    }

    /// 在模块初始化时创建`GovernmentCapability`和`LandRegistry`对象
    /// Create a `LandRegistry` on module init.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(GovernmentCapability {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        transfer::share_object(LandRegistry {
            id: object::new(ctx),
            balance: balance::zero<SUI>(),
            fee: 10000
        })
    }

    /// 为产权所有人(`for`)创建`TitleDeed`, 只有`GovernmentCapability`的
    /// 所有者才可以创建
    /// Create `TitleDeed` and transfer it to the property owner.
    /// Only owner of the `GovernmentCapability` can perform this action.
    public entry fun issue_title_deed(
        _: &GovernmentCapability,
        for: address,
        ctx: &mut TxContext
    ) {
        transfer::transfer(TitleDeed {
            id: object::new(ctx)
        }, for)
    }

    /// 因为`TitleDeed`没有`store` ability 所以需要一个自定义的转移函数来变更所有权。
    /// 在这个例子中所有变更`TitleDeed`所有权的操作都需要支付`registry.fee`。
    /// A custom transfer function. Required due to `TitleDeed` not having
    /// a `store` ability. All transfers of `TitleDeed`s have to go through
    /// this function and pay a fee to the `LandRegistry`.
    public entry fun transfer_ownership(
        registry: &mut LandRegistry,
        paper: TitleDeed,
        fee: Coin<SUI>,
        to: address,
    ) {
        assert!(coin::value(&fee) == registry.fee, EWrongAmount);

        // 将支付的费用转给`LandRegistry`
        // add a payment to the LandRegistry balance
        balance::join(&mut registry.balance, coin::into_balance(fee));

        // 调用转移函数`transfer` 完成所有权变更
        // finally call the transfer function
        transfer::transfer(paper, to)
    }
}
