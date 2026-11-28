// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Unfreezing lifts the freeze once no SeizeAuth is outstanding. A permanent freeze is never lifted.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::regulated_coin;

use sui::coin_registry;

public struct REGULATED_COIN has drop {}

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
    let cap = currency.make_compliance_regulated(true, ctx);
    let metadata_cap = currency.finalize(ctx);
    let coin = treasury_cap.mint(10000, ctx);
    transfer::public_transfer(coin, ctx.sender());
    transfer::public_transfer(cap, ctx.sender());
    transfer::public_transfer(metadata_cap, ctx.sender());
    transfer::public_freeze_object(treasury_cap);
}

//# run sui::coin_registry::finalize_registration --args object(0xc) receiving(1,2) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(0xcf) object(1,3) @B @A
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3))

// Unfreezing waits for every SeizeAuth to be destroyed.
//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin_regulation::destroy_seize_auth --args object(0xcf) object(8,0) --type-args test::regulated_coin::REGULATED_COIN --sender A

// Unfreezing waits for an epoch after the last SeizeAuth destroy.
//# advance-epoch

//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @B --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @B
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

//# run sui::coin_regulation::freeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(0xcf) object(1,3) @C @A
//> 0: sui::coin_regulation::issue_seize_auth<test::regulated_coin::REGULATED_COIN>(Input(0), Input(1), Input(2));
//> sui::coin_regulation::transfer_seize_auth<test::regulated_coin::REGULATED_COIN>(Result(0), Input(3))

//# run sui::coin_regulation::permanently_freeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))

// The SeizeAuth input does not exempt C.
//# programmable --sender A --inputs object(15,0) object(1,5) 1 @C
//> 0: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0)], Input(3))

//# run sui::coin_regulation::destroy_seize_auth --args object(0xcf) object(15,0) --type-args test::regulated_coin::REGULATED_COIN --sender A

//# run sui::coin_regulation::unfreeze_address --args object(0xcf) object(1,3) @C --type-args test::regulated_coin::REGULATED_COIN --sender A

//# programmable --sender A --inputs object(1,5) 1 @C
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
