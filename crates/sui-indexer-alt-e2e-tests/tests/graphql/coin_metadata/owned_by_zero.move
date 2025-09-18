// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P=0x0 --accounts A --simulator

//# publish --sender A
#[allow(deprecated_usage)]
module P::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(witness, 2, b"FAKE", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, @0x0);
    }
}

//# create-checkpoint

//# run-graphql
{
  coinMetadata(coinType: "@{P}::fake::FAKE") {
    decimals
    name
    symbol
    description
    iconUrl
    supply
    supplyState
  }
}
