// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::treasury {
    use std::type_name;

    use sui::coin::{Self, Coin};
    use sui::object_bag::{Self, ObjectBag};
    use sui::tx_context::{Self, TxContext};

    use bridge::btc;
    use bridge::btc::BTC;
    use bridge::eth;
    use bridge::eth::ETH;
    use bridge::usdc;
    use bridge::usdc::USDC;
    use bridge::usdt;
    use bridge::usdt::USDT;

    const EUnsupportedTokenType: u64 = 0;
    const ENotSystemAddress: u64 = 1;

    public struct BridgeTreasury has store {
        treasuries: ObjectBag
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

    public fun decimal_multiplier<T>(): u64 {
        let coin_type = type_name::get<T>();
        if (coin_type == type_name::get<BTC>()) {
            btc::multiplier()
        } else if (coin_type == type_name::get<ETH>()) {
            eth::multiplier()
        } else if (coin_type == type_name::get<USDC>()) {
            usdc::multiplier()
        } else if (coin_type == type_name::get<USDT>()) {
            usdt::multiplier()
        } else {
            abort EUnsupportedTokenType
        }
    }

    public(package) fun create(ctx: &mut TxContext): BridgeTreasury {
        assert!(tx_context::sender(ctx) == @0x0, ENotSystemAddress);
        BridgeTreasury {
            treasuries: object_bag::new(ctx)
        }
    }

    public(package) fun burn<T>(self: &mut BridgeTreasury, token: Coin<T>, ctx: &mut TxContext) {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = object_bag::borrow_mut(&mut self.treasuries, type_name::get<T>());
        coin::burn(treasury, token);
    }

    public(package) fun mint<T>(self: &mut BridgeTreasury, amount: u64, ctx: &mut TxContext): Coin<T> {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = object_bag::borrow_mut(&mut self.treasuries, type_name::get<T>());
        coin::mint(treasury, amount, ctx)
    }

    fun create_treasury_if_not_exist<T>(self: &mut BridgeTreasury, ctx: &mut TxContext) {
        let type_ = type_name::get<T>();
        if (!object_bag::contains(&self.treasuries, type_)) {
            // Lazily create currency if not exists
            if (type_ == type_name::get<BTC>()) {
                object_bag::add(&mut self.treasuries, type_, btc::create(ctx));
            } else if (type_ == type_name::get<ETH>()) {
                object_bag::add(&mut self.treasuries, type_, eth::create(ctx));
            } else if (type_ == type_name::get<USDC>()) {
                object_bag::add(&mut self.treasuries, type_, usdc::create(ctx));
            } else if (type_ == type_name::get<USDT>()) {
                object_bag::add(&mut self.treasuries, type_, usdt::create(ctx));
            } else {
                abort EUnsupportedTokenType
            };
        };
    }
}
