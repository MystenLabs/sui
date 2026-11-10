// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Migration freezes the addresses in its vector that are denied in the legacy next-epoch list,
// including a pending add, and skips the rest. The FreezeList applies in the same epoch.

//# init --accounts A B C D E --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin;
use sui::coin_regulation::{Self, FreezeList};

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

public fun assert_frozen(list: &FreezeList, addr: address, expected: bool) {
    assert!(coin_regulation::is_frozen<REGULATED_COIN>(list, addr) == expected);
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# advance-epoch

// C is pending removal and D is a pending add: C is still denied as a receiver this epoch, D is not.
//# run sui::coin::deny_list_v2_remove --args object(0x403) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @D --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 1 @D
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@B,@C,@D,@E] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

//# run test::regulated_coin::assert_frozen --args immshared(0xcf) @B true --sender A

//# run test::regulated_coin::assert_frozen --args immshared(0xcf) @C false --sender A

//# run test::regulated_coin::assert_frozen --args immshared(0xcf) @D true --sender A

//# run test::regulated_coin::assert_frozen --args immshared(0xcf) @E false --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 1 @D
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# programmable --sender A --inputs object(1,5) 1 @E
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
