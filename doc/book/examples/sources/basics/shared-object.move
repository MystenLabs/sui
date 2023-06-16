// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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

    /// For when Coin balance is too low.
    const ENotEnough: u64 = 0;

    /// Capability that grants an owner the right to collect profits.
    struct ShopOwnerCap has key { id: UID }

    /// A purchasable Donut. For simplicity's sake we ignore implementation.
    struct Donut has key { id: UID }

    /// A shared object. `key` ability is required.
    struct DonutShop has key {
        id: UID,
        price: u64,
        balance: Balance<SUI>
    }

    #[allow(unused_function)]
    /// Init function is often ideal place for initializing
    /// a shared object as it is called only once.
    ///
    /// To share an object `transfer::share_object` is used.
    fun init(ctx: &mut TxContext) {
        transfer::transfer(ShopOwnerCap {
            id: object::new(ctx)
        }, tx_context::sender(ctx));

        // Share the object to make it accessible to everyone!
        transfer::share_object(DonutShop {
            id: object::new(ctx),
            price: 1000,
            balance: balance::zero()
        })
    }

    /// Entry function available to everyone who owns a Coin.
    public entry fun buy_donut(
        shop: &mut DonutShop, payment: &mut Coin<SUI>, ctx: &mut TxContext
    ) {
        assert!(coin::value(payment) >= shop.price, ENotEnough);

        // Take amount = `shop.price` from Coin<SUI>
        let coin_balance = coin::balance_mut(payment);
        let paid = balance::split(coin_balance, shop.price);

        // Put the coin to the Shop's balance
        balance::join(&mut shop.balance, paid);

        transfer::transfer(Donut {
            id: object::new(ctx)
        }, tx_context::sender(ctx))
    }

    /// Consume donut and get nothing...
    public entry fun eat_donut(d: Donut) {
        let Donut { id } = d;
        object::delete(id);
    }

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
