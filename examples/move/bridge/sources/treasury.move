// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::treasury {
    use std::type_name;

    use sui::coin::{Self, Coin};
    use sui::object_bag::{Self, ObjectBag};
    use sui::tx_context::TxContext;

    use bridge::btc::BTC;
    use bridge::eth::ETH;
    use bridge::usdc::USDC;
    use bridge::usdt::USDT;
    use sui::coin::TreasuryCap;

    friend bridge::bridge;

    const EUnsupportedTokenType: u64 = 0;
    // const ENotSystemAddress: u64 = 1;

    struct BridgeTreasury has store {
        treasuries: ObjectBag
    }

    public(friend) fun add_treasury_cap<T>(self: &mut BridgeTreasury, treasury_cap: TreasuryCap<T>) {
        let type = type_name::get<T>();
        object_bag::add(&mut self.treasuries, type, treasury_cap)
    }

    public(friend) fun create(ctx: &mut TxContext): BridgeTreasury {
        // assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        BridgeTreasury {
            treasuries: object_bag::new(ctx)
        }
    }

    public(friend) fun burn<T>(self: &mut BridgeTreasury, token: Coin<T>, ctx: &TxContext) {
    // public(friend) fun burn<T>(self: &mut BridgeTreasury, token: Coin<T>, ctx: &mut TxContext) {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = object_bag::borrow_mut(&mut self.treasuries, type_name::get<T>());
        coin::burn(treasury, token);
    }

    public(friend) fun mint<T>(self: &mut BridgeTreasury, amount: u64, ctx: &mut TxContext): Coin<T> {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = object_bag::borrow_mut(&mut self.treasuries, type_name::get<T>());
        coin::mint(treasury, amount, ctx)
    }

    // fun create_treasury_if_not_exist<T>(self: &mut BridgeTreasury, ctx: &mut TxContext) {
    fun create_treasury_if_not_exist<T>(self: &BridgeTreasury, _ctx: &TxContext) {
        let type = type_name::get<T>();
        if (!object_bag::contains(&self.treasuries, type)) {
            // // Lazily create currency if not exists
            // if (type == type_name::get<BTC>()) {
            //     object_bag::add(&mut self.treasuries, type, btc::create(ctx));
            // } else if (type == type_name::get<ETH>()) {
            //     object_bag::add(&mut self.treasuries, type, eth::create(ctx));
            // } else if (type == type_name::get<USDC>()) {
            //     object_bag::add(&mut self.treasuries, type, usdc::create(ctx));
            // } else if (type == type_name::get<USDT>()) {
            //     object_bag::add(&mut self.treasuries, type, usdt::create(ctx));
            // } else {
                abort EUnsupportedTokenType
            // };
        };
    }

    public fun token_id<T>(): u8 {
        let coin_type = type_name::get<T>();
        if (coin_type == type_name::get<BTC>()) {
            1
        } else if (coin_type == type_name::get<ETH>()) {
            2
        } else if (coin_type == type_name::get<USDC>()) {
            3
        } else if (coin_type == type_name::get<USDT>()) {
            4
        } else {
            abort EUnsupportedTokenType
        }
    }
}
