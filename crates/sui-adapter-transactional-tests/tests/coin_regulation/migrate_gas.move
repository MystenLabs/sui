// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Migrating a coin type does not change the gas charged for its coin owners, though the legacy deny
// list no longer checks them.

//# init --accounts A --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::address;
use sui::coin::{Self, Coin};

public struct REGULATED_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A regulated coin",
        option::none(),
        true,
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 100000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

public fun send_100(coin: &mut Coin<REGULATED_COIN>, ctx: &mut TxContext) {
    let mut i = 0u64;
    while (i < 100) {
        transfer::public_transfer(coin.split(1, ctx), address::from_u256(i as u256));
        i = i + 1;
    }
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run test::regulated_coin::send_100 --args object(1,5) --sender A

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

// The same computation cost as before the migration.
//# run test::regulated_coin::send_100 --args object(1,5) --sender A
