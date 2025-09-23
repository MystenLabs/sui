// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P=0x0 --accounts A --simulator

//# publish --sender A
#[allow(deprecated_usage)]
module P::fake {
    use sui::coin::{Self, DenyCapV2};

    public struct FAKE has drop {}

    public struct Wrapper has key, store {
        id: UID,
        cap: DenyCapV2<FAKE>,
    }

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (treasury_cap, deny_cap, metadata) = coin::create_regulated_currency_v2(
          witness,
          2,
          b"FAKE",
          b"",
          b"",
          option::none(),
          false,
          ctx
        );

        let wrapper = Wrapper {
            id: object::new(ctx),
            cap: deny_cap,
        };

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(wrapper, ctx.sender());
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
    regulatedState
    allowGlobalPause
    denyCap {
      contents {
        type { repr }
        json
      }
    }
  }
}
