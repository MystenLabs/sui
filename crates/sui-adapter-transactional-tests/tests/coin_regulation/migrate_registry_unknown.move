// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Migrating a legacy coin whose `Currency<T>` regulated state is `Unknown` sets it to
// `Regulated` with the cap's ID.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;
use sui::coin_registry::Currency;
use sui::coin_regulation::ComplianceCap;

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
        false,
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

public fun assert_compliance_cap(
    currency: &Currency<REGULATED_COIN>,
    cap: &ComplianceCap<REGULATED_COIN>,
) {
    assert!(currency.is_regulated());
    assert!(currency.deny_cap_id() == option::some(object::id(cap)));
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# view-object 2,0

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@B] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# view-object 2,0

//# run test::regulated_coin::assert_compliance_cap --args immshared(2,0) object(1,3) --sender A

// The regulated state is set, so the permissionless metadata migration aborts.
//# run sui::coin_registry::migrate_regulated_state_by_metadata --args object(2,0) object(1,4) --type-args test::regulated_coin::REGULATED_COIN --sender A
