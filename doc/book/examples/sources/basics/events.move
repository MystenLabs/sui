// Copyright (c) 2022, Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

/// Extended example of a shared object. Now with addition of events!
module examples::donuts_with_events {
    use sui::transfer;
    use sui::sui::SUI;
    use sui::coin::{Self, Coin};
    use sui::object::{Self, ID, Info};
    use sui::balance::{Self, Balance};
    use sui::tx_context::{Self, TxContext};

    // This is the only dependency you need for events.
    use sui::event;

    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// Capability that grants an owner the right to collect profits.
    struct ShopOwnerCap has key { info: Info }

    /// A purchasable Donut. For simplicity's sake we ignore implementation.
    struct Donut has key { info: Info }

    struct DonutShop has key {
        info: Info,
        price: u64,
        balance: Balance<SUI>
    }

    // ====== Events ======

    /// For when someone has purchased a donut.
    struct DonutBought has copy, drop {
        id: ID
    }

    /// For when DonutShop owner has collected profits.
    struct ProfitsCollected has copy, drop {
        amount: u64
    }

    // ====== Functions ======

    fun init(ctx: &mut TxContext) {
        transfer::transfer(ShopOwnerCap {
            info: object::new(ctx)
        }, tx_context::sender(ctx));

        transfer::share_object(DonutShop {
            info: object::new(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// Buy a donut.
    public entry fun buy_donut(
        shop: &mut DonutShop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);
        let info = object::new(ctx);

        balance::join(&mut shop.balance, paid);

        // Emit the event using future object's ID.
        event::emit(DonutBought { id: *object::info_id(&info) });
        transfer::transfer(Donut { info }, tx_context::sender(ctx))
    }

    /// Consume donut and get nothing...
    public entry fun eat_donut(d: Donut) {
        let Donut { info } = d;
        object::delete(info);
    }

    /// Take coin from `DonutShop` and transfer it to tx sender.
    /// Requires authorization with `ShopOwnerCap`.
    public entry fun collect_profits(
        _: &ShopOwnerCap, shop: &mut DonutShop, ctx: &mut TxContext
    ) {
        let amount = balance::value(&shop.balance);
        let profits = coin::take(&mut shop.balance, amount, ctx);

        // simply create new type instance and emit it
        event::emit(ProfitsCollected { amount });

        transfer::transfer(profits, tx_context::sender(ctx))
    }
}
