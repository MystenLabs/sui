// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module bridge::treasury;

use std::ascii::{Self, String};
use std::type_name::{Self, TypeName};
use sui::address;
use sui::bag::{Self, Bag};
use sui::coin::{Self, Coin, TreasuryCap, CoinMetadata};
use sui::event;
use sui::hex;
use sui::object_bag::{Self, ObjectBag};
use sui::package::{Self, UpgradeCap};
use sui::vec_map::{Self, VecMap};

const EUnsupportedTokenType: u64 = 1;
const EInvalidUpgradeCap: u64 = 2;
const ETokenSupplyNonZero: u64 = 3;
const EInvalidNotionalValue: u64 = 4;

#[test_only]
const USD_VALUE_MULTIPLIER: u64 = 100000000; // 8 DP accuracy

//////////////////////////////////////////////////////
// Types
//

public struct BridgeTreasury has store {
    // token treasuries, values are TreasuryCaps for native bridge V1.
    treasuries: ObjectBag,
    supported_tokens: VecMap<TypeName, BridgeTokenMetadata>,
    // Mapping token id to type name
    id_token_type_map: VecMap<u8, TypeName>,
    // Bag for storing potential new token waiting to be approved
    waiting_room: Bag,
}

public struct BridgeTokenMetadata has copy, drop, store {
    id: u8,
    decimal_multiplier: u64,
    notional_value: u64,
    native_token: bool,
}

public struct ForeignTokenRegistration has store {
    type_name: TypeName,
    uc: UpgradeCap,
    decimal: u8,
}

public struct UpdateTokenPriceEvent has copy, drop {
    token_id: u8,
    new_price: u64,
}

public struct NewTokenEvent has copy, drop {
    token_id: u8,
    type_name: TypeName,
    native_token: bool,
    decimal_multiplier: u64,
    notional_value: u64,
}

public struct TokenRegistrationEvent has copy, drop {
    type_name: TypeName,
    decimal: u8,
    native_token: bool,
}

public fun token_id<T>(self: &BridgeTreasury): u8 {
    let metadata = self.get_token_metadata<T>();
    metadata.id
}

public fun decimal_multiplier<T>(self: &BridgeTreasury): u64 {
    let metadata = self.get_token_metadata<T>();
    metadata.decimal_multiplier
}

public fun notional_value<T>(self: &BridgeTreasury): u64 {
    let metadata = self.get_token_metadata<T>();
    metadata.notional_value
}

//////////////////////////////////////////////////////
// Internal functions
//

public(package) fun register_foreign_token<T>(
    self: &mut BridgeTreasury,
    tc: TreasuryCap<T>,
    uc: UpgradeCap,
    metadata: &CoinMetadata<T>,
) {
    // Make sure TreasuryCap has not been minted before.
    assert!(coin::total_supply(&tc) == 0, ETokenSupplyNonZero);
    let type_name = type_name::get<T>();
    let address_bytes = hex::decode(ascii::into_bytes(type_name::get_address(&type_name)));
    let coin_address = address::from_bytes(address_bytes);
    // Make sure upgrade cap is for the Coin package
    // FIXME: add test
    assert!(
        object::id_to_address(&package::upgrade_package(&uc)) == coin_address,
        EInvalidUpgradeCap,
    );
    let registration = ForeignTokenRegistration {
        type_name,
        uc,
        decimal: coin::get_decimals(metadata),
    };
    self.waiting_room.add(type_name::into_string(type_name), registration);
    self.treasuries.add(type_name, tc);

    event::emit(TokenRegistrationEvent {
        type_name,
        decimal: coin::get_decimals(metadata),
        native_token: false,
    });
}

public(package) fun add_new_token(
    self: &mut BridgeTreasury,
    token_name: String,
    token_id: u8,
    native_token: bool,
    notional_value: u64,
) {
    if (!native_token) {
        assert!(notional_value > 0, EInvalidNotionalValue);
        let ForeignTokenRegistration {
            type_name,
            uc,
            decimal,
        } = self.waiting_room.remove<String, ForeignTokenRegistration>(token_name);
        let decimal_multiplier = 10u64.pow(decimal);
        self
            .supported_tokens
            .insert(
                type_name,
                BridgeTokenMetadata {
                    id: token_id,
                    decimal_multiplier,
                    notional_value,
                    native_token,
                },
            );
        self.id_token_type_map.insert(token_id, type_name);

        // Freeze upgrade cap to prevent changes to the coin
        transfer::public_freeze_object(uc);

        event::emit(NewTokenEvent {
            token_id,
            type_name,
            native_token,
            decimal_multiplier,
            notional_value,
        })
    } // else not implemented in V1
}

public(package) fun create(ctx: &mut TxContext): BridgeTreasury {
    BridgeTreasury {
        treasuries: object_bag::new(ctx),
        supported_tokens: vec_map::empty(),
        id_token_type_map: vec_map::empty(),
        waiting_room: bag::new(ctx),
    }
}

public(package) fun burn<T>(self: &mut BridgeTreasury, token: Coin<T>) {
    let treasury = &mut self.treasuries[type_name::get<T>()];
    coin::burn(treasury, token);
}

public(package) fun mint<T>(self: &mut BridgeTreasury, amount: u64, ctx: &mut TxContext): Coin<T> {
    let treasury = &mut self.treasuries[type_name::get<T>()];
    coin::mint(treasury, amount, ctx)
}

public(package) fun update_asset_notional_price(
    self: &mut BridgeTreasury,
    token_id: u8,
    new_usd_price: u64,
) {
    let type_name = self.id_token_type_map.try_get(&token_id);
    assert!(type_name.is_some(), EUnsupportedTokenType);
    assert!(new_usd_price > 0, EInvalidNotionalValue);
    let type_name = type_name.destroy_some();
    let metadata = self.supported_tokens.get_mut(&type_name);
    metadata.notional_value = new_usd_price;

    event::emit(UpdateTokenPriceEvent {
        token_id,
        new_price: new_usd_price,
    })
}

fun get_token_metadata<T>(self: &BridgeTreasury): BridgeTokenMetadata {
    let coin_type = type_name::get<T>();
    let metadata = self.supported_tokens.try_get(&coin_type);
    assert!(metadata.is_some(), EUnsupportedTokenType);
    metadata.destroy_some()
}

//////////////////////////////////////////////////////
// Test functions
//

#[test_only]
public struct ETH has drop {}
#[test_only]
public struct BTC has drop {}
#[test_only]
public struct USDT has drop {}
#[test_only]
public struct USDC has drop {}

#[test_only]
public fun new_for_testing(ctx: &mut TxContext): BridgeTreasury {
    create(ctx)
}

#[test_only]
public fun mock_for_test(ctx: &mut TxContext): BridgeTreasury {
    let mut treasury = new_for_testing(ctx);
    treasury.setup_for_testing();
    treasury
}

#[test_only]
public fun setup_for_testing(treasury: &mut BridgeTreasury) {
    treasury
        .supported_tokens
        .insert(
            type_name::get<BTC>(),
            BridgeTokenMetadata {
                id: 1,
                decimal_multiplier: 100_000_000,
                notional_value: 50_000 * USD_VALUE_MULTIPLIER,
                native_token: false,
            },
        );
    treasury
        .supported_tokens
        .insert(
            type_name::get<ETH>(),
            BridgeTokenMetadata {
                id: 2,
                decimal_multiplier: 100_000_000,
                notional_value: 3_000 * USD_VALUE_MULTIPLIER,
                native_token: false,
            },
        );
    treasury
        .supported_tokens
        .insert(
            type_name::get<USDC>(),
            BridgeTokenMetadata {
                id: 3,
                decimal_multiplier: 1_000_000,
                notional_value: USD_VALUE_MULTIPLIER,
                native_token: false,
            },
        );
    treasury
        .supported_tokens
        .insert(
            type_name::get<USDT>(),
            BridgeTokenMetadata {
                id: 4,
                decimal_multiplier: 1_000_000,
                notional_value: USD_VALUE_MULTIPLIER,
                native_token: false,
            },
        );

    treasury.id_token_type_map.insert(1, type_name::get<BTC>());
    treasury.id_token_type_map.insert(2, type_name::get<ETH>());
    treasury.id_token_type_map.insert(3, type_name::get<USDC>());
    treasury.id_token_type_map.insert(4, type_name::get<USDT>());
}

#[test_only]
public fun waiting_room(treasury: &BridgeTreasury): &Bag {
    &treasury.waiting_room
}

#[test_only]
public fun treasuries(treasury: &BridgeTreasury): &ObjectBag {
    &treasury.treasuries
}

#[test_only]
public fun unwrap_update_event(event: UpdateTokenPriceEvent): (u8, u64) {
    (event.token_id, event.new_price)
}

#[test_only]
public fun unwrap_new_token_event(event: NewTokenEvent): (u8, TypeName, bool, u64, u64) {
    (
        event.token_id,
        event.type_name,
        event.native_token,
        event.decimal_multiplier,
        event.notional_value,
    )
}

#[test_only]
public fun unwrap_registration_event(event: TokenRegistrationEvent): (TypeName, u8, bool) {
    (event.type_name, event.decimal, event.native_token)
}
