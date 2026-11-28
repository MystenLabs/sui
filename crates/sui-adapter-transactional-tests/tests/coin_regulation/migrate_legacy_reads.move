// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Every legacy deny list read aborts for a migrated coin type. The mutating wrappers need the
// DenyCapV2 that migration consumes, so they are unreachable here.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;

public struct REGULATED_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        true,
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_contains_current_epoch --args immshared(0x403) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_contains_next_epoch --args immshared(0x403) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_is_global_pause_enabled_current_epoch --args immshared(0x403) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_is_global_pause_enabled_next_epoch --args immshared(0x403) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@B] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# run sui::coin::deny_list_v2_contains_current_epoch --args immshared(0x403) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_contains_next_epoch --args immshared(0x403) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_is_global_pause_enabled_current_epoch --args immshared(0x403) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_is_global_pause_enabled_next_epoch --args immshared(0x403) --type-args test::regulated_coin::REGULATED_COIN --sender A
