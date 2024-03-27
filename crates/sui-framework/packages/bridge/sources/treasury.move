// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::treasury {
    use std::type_name;
    use sui::coin::{Self, Coin};
    use sui::object_bag::{Self, ObjectBag};

    use bridge::btc::{Self, BTC};
    use bridge::eth::{Self, ETH};
    use bridge::usdc::{Self, USDC};
    use bridge::usdt::{Self, USDT};

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
        assert!(ctx.sender() == @0x0, ENotSystemAddress);
        BridgeTreasury {
            treasuries: object_bag::new(ctx)
        }
    }

    public(package) fun burn<T>(self: &mut BridgeTreasury, token: Coin<T>, ctx: &mut TxContext) {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = &mut self.treasuries[type_name::get<T>()];
        coin::burn(treasury, token);
    }

    public(package) fun mint<T>(self: &mut BridgeTreasury, amount: u64, ctx: &mut TxContext): Coin<T> {
        create_treasury_if_not_exist<T>(self, ctx);
        let treasury = &mut self.treasuries[type_name::get<T>()];
        coin::mint(treasury, amount, ctx)
    }

    fun create_treasury_if_not_exist<T>(self: &mut BridgeTreasury, ctx: &mut TxContext) {
        let type_ = type_name::get<T>();
        if (!self.treasuries.contains(type_)) {
            // Lazily create currency if not exists
            if (type_ == type_name::get<BTC>()) {
                self.treasuries.add(type_, btc::create(ctx));
            } else if (type_ == type_name::get<ETH>()) {
                self.treasuries.add(type_, eth::create(ctx));
            } else if (type_ == type_name::get<USDC>()) {
                self.treasuries.add(type_, usdc::create(ctx));
            } else if (type_ == type_name::get<USDT>()) {
                self.treasuries.add(type_, usdt::create(ctx));
            } else {
                abort EUnsupportedTokenType
            };
        };
    }
}
