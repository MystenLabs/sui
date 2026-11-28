// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// The ComplianceCap keeps the DenyCapV2's object ID, so the `Currency<T>` registry entry still
// names the cap.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin::DenyCapV2;
use sui::coin_registry::{Self, Currency};
use sui::coin_regulation::ComplianceCap;

public struct REGULATED_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut currency, mut treasury_cap) = coin_registry::new_currency_with_otw(
        otw,
        9,
        b"RC".to_string(),
        b"REGULATED_COIN".to_string(),
        b"A new regulated coin".to_string(),
        b"".to_string(),
        ctx,
    );
    let deny_cap = currency.make_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

public fun assert_deny_cap(currency: &Currency<REGULATED_COIN>, cap: &DenyCapV2<REGULATED_COIN>) {
    assert!(currency.deny_cap_id() == option::some(object::id(cap)));
}

public fun assert_compliance_cap(
    currency: &Currency<REGULATED_COIN>,
    cap: &ComplianceCap<REGULATED_COIN>,
) {
    assert!(currency.deny_cap_id() == option::some(object::id(cap)));
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# view-object 1,3

//# run test::regulated_coin::assert_deny_cap --args immshared(2,0) object(1,3) --sender A

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@B] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# view-object 1,3

//# view-object 2,0

//# run test::regulated_coin::assert_compliance_cap --args immshared(2,0) object(1,3) --sender A

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A
