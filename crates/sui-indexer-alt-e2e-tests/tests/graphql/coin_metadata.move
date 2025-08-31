// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P=0x0 --accounts A --simulator

//# publish --sender A
module P::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, metadata) = coin::create_currency(witness, 2, b"FAKE", b"", b"", option::none(), ctx);
        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, ctx.sender());
    }
}

//# create-checkpoint

//# run-graphql
{
  transactionEffects(digest: "@{digest_1}") {
    objectChanges {
      nodes {
        outputState {
          asMoveObject {
            asCoinMetadata { ...CM }
          }
        }
      }
    }
  }

  objects(filter: { type: "0x2::coin::CoinMetadata<@{P}::fake::FAKE>" }) {
    nodes {
      asMoveObject {
        asCoinMetadata { ...CM }
      }
    }
  }

  fake: coinMetadata(coinType: "@{P}::fake::FAKE") { ...CM }

  sui: coinMetadata(coinType: "0x2::sui::SUI") { ...CM }
}

fragment CM on CoinMetadata {
  decimals
  name
  symbol
  description
  iconUrl
  supply
}

//# programmable --sender A --inputs object(1,2) 100 @A
//> 0: sui::coin::mint<P::fake::FAKE>(Input(0), Input(1));
//> TransferObjects([Result(0)], Input(2))

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
  }
}
