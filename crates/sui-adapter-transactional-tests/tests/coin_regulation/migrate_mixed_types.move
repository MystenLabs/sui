// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// One transaction touches the migrated COIN_A and the legacy COIN_B. The legacy deny list still
// applies to COIN_B, and only the FreezeList applies to COIN_A.

//# init --accounts A B C --addresses test=0x0

//# publish --sender A
module test::coin_a {
    use sui::coin;

    public struct COIN_A has drop {}

    #[allow(deprecated_usage)]
    fun init(otw: COIN_A, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"A",
            b"COIN_A",
            b"A regulated coin",
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
}

module test::coin_b {
    use sui::coin;

    public struct COIN_B has drop {}

    #[allow(deprecated_usage)]
    fun init(otw: COIN_B, ctx: &mut TxContext) {
        let (mut treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
            otw,
            9,
            b"B",
            b"COIN_B",
            b"Another regulated coin",
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
}

//# run sui::coin_registry::migrate_legacy_metadata --args object(0xc) object(1,2) --type-args test::coin_a::COIN_A --sender A

//# programmable --sender A --inputs object(1,5) object(1,10) 10 @B
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0), Result(1)], Input(3))

//# programmable --sender A --inputs object(1,5) object(1,10) 10 @C
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0), Result(1)], Input(3))

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @B --type-args test::coin_a::COIN_A --sender A

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,8) @B --type-args test::coin_b::COIN_B --sender A

//# run sui::coin::deny_list_v2_add --args object(0x403) object(1,3) @C --type-args test::coin_a::COIN_A --sender A

//# advance-epoch

// Migration freezes the listed C for COIN_A. B is not listed, and its legacy COIN_A entry is
// ignored.
//# programmable --sender A --inputs object(2,0) object(0xcf) object(0x403) object(1,3) vector[@C] @A
//> 0: sui::coin_registry::migrate_to_compliance_cap<test::coin_a::COIN_A>(Input(0), Input(1), Input(2), Input(3), Input(4));
//> TransferObjects([Result(0)], Input(5))

// Execution denies the recipient B for COIN_B. The deny list checks COIN_A first, so this shows
// B's COIN_A entry is ignored.
//# programmable --sender A --inputs object(1,5) object(1,10) 1 @B
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0), Result(1)], Input(3))

// Signing denies the sender B for COIN_B, also past its ignored COIN_A entry.
//# programmable --sender B --inputs object(3,0) object(3,1) 1 @A
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0), Result(1)], Input(3))

// Signing denies the sender C for COIN_A through the FreezeList.
//# programmable --sender C --inputs object(4,0) object(4,1) 1 @A
//> 0: SplitCoins(Input(0), [Input(2)]);
//> 1: SplitCoins(Input(1), [Input(2)]);
//> TransferObjects([Result(0), Result(1)], Input(3))

// C is frozen only for COIN_A, so it can move COIN_B.
//# programmable --sender C --inputs object(4,1) 1 @A
//> 0: SplitCoins(Input(0), [Input(1)]);
//> TransferObjects([Result(0)], Input(2))
