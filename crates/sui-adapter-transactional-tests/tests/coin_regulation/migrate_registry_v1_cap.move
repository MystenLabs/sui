// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// A coin first regulated with a v1 `DenyCap` and marked through its `RegulatedCoinMetadata` names
// the deleted v1 cap. Migration replaces that ID with the ComplianceCap's in one step.

//# init --accounts A B --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;

public struct REGULATED_COIN has drop {}

#[allow(deprecated_usage)]
fun init(otw: REGULATED_COIN, ctx: &mut TxContext) {
    let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency(
        otw,
        9,
        b"RC",
        b"REGULATED_COIN",
        b"A new regulated coin",
        option::none(),
        ctx,
    );
    let coin = coin::mint(&mut treasury_cap, 10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(deny_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
    transfer::public_freeze_object(metadata);
}

//# programmable --sender A --inputs object(0x403) object(1,3) true @A
//> 0: sui::coin::migrate_regulated_currency_to_v2<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> TransferObjects([Result(0)], Input(3))

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin_registry::migrate_regulated_state_by_metadata --args object(3,0) object(1,4) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# view-object 3,0

//# programmable --sender A --inputs object(3,0) object(0xcf) object(0x403) object(2,1) vector[@B] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# view-object 2,1

//# view-object 3,0
