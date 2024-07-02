// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// 与`Owned`对象不同，`Shared`对象可以被任何人使用， 因此需要根据需求在逻辑中设计额外的安全检查
/// Unlike `Owned` objects, `Shared` ones can be accessed by anyone on the
/// network. Extended functionality and accessibility of this kind of objects
/// requires additional effort by securing access if needed.
module examples::donuts {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, UID};
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};

    /// 错误码 0：对应所付费用低于甜甜圈价格
    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// 商店所有者权限凭证：获取利润
    /// Capability that grants an owner the right to collect profits.
    struct ShopOwnerCap has key { id: UID }

    /// 一个可被购买的甜甜圈对象
    /// A purchasable Donut. For simplicity's sake we ignore implementation.
    struct Donut has key { id: UID }

    /// 一个共享对象（需要具备`key`）
    /// A shared object. `key` ability is required.
    struct DonutShop has key {
        id: UID,
        price: u64,
        balance: Balance<SUI>
    }

    /// 通常可以在在初始化函数中创建共享对象，因为初始化函数只被执行一次
    /// Init function is often ideal place for initializing
    /// a shared object as it is called only once.
    /// 使用`transfer::share_object`共享对象
    /// To share an object `transfer::share_object` is used.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(ShopOwnerCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        // 通过共享对象使其可以被任何人使用
        // Share the object to make it accessible to everyone!
        transfer::share_object(DonutShop {
            id: object::new(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// 任何拥有`Coin`的用户都可以调用`buy_donut`入口函数
    /// Entry function available to everyone who owns a Coin.
    public entry fun buy_donut(
        shop: &mut DonutShop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        // 从Coin<SUI>分离 amount = `shop.price`的对象
        // Take amount = `shop.price` from Coin<SUI>
        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);

        // 存入商店的收支中
        // Put the coin to the Shop's balance
        balance::join(&mut shop.balance, paid);

        transfer::transfer(Donut {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// 吃掉甜甜圈 ：）
    /// Consume donut and get nothing...
    public entry fun eat_donut(d: Donut) {
        let Donut { id } = d;
        object::delete(id);
    }

    /// 收集利润，需要`ShopOwnerCap`凭证
    /// Take coin from `DonutShop` and transfer it to tx sender.
    /// Requires authorization with `ShopOwnerCap`.
    public entry fun collect_profits(
        _: &ShopOwnerCap, shop: &mut DonutShop, ctx: &mut TxContext
    ) {
        let amount = balance::value(&shop.balance);
        let profits = coin::take(&mut shop.balance, amount, ctx);

        transfer::public_transfer(profits, tx_context::sender(ctx))
    }
}
