// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --accounts A --addresses ex=0x0

//# publish --sender A
module ex::m;

use sui::coin;

public struct M has drop {}

fun init(witness: M, ctx: &mut TxContext) {
    let (treasury_cap, metadata) = coin::create_currency(witness, 2, b"M", b"", b"", option::none(), ctx);
    transfer::public_freeze_object(metadata);
    transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
}

// Verify ConsenusAddressOwner coin can be merged into AddressOwner coin.
//# programmable --sender A --inputs object(1,2) 100 @A
//> 0: sui::coin::mint<ex::m::M>(Input(0), Input(1));
//> 1: sui::coin::mint<ex::m::M>(Input(0), Input(1));
//> 2: TransferObjects([Result(0)], Input(2));
//> 3: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<ex::m::M>>(Result(1), Result(3))

//# programmable --sender A --inputs object(2,0) object(2,1) @A
//> MergeCoins(Input(1), [Input(0)])

//# view-object 2,1

// Verify AddressOwner coin can be merged into ConsensusAddressOwner coin.
//# programmable --sender A --inputs object(1,2) 100 @A
//> 0: sui::coin::mint<ex::m::M>(Input(0), Input(1));
//> 1: sui::coin::mint<ex::m::M>(Input(0), Input(1));
//> 2: TransferObjects([Result(0)], Input(2));
//> 3: sui::party::single_owner(Input(2));
//> sui::transfer::public_party_transfer<sui::coin::Coin<ex::m::M>>(Result(1), Result(3))

//# programmable --sender A --inputs object(5,0) object(5,1) @A
//> MergeCoins(Input(0), [Input(1)])

//# view-object 5,0