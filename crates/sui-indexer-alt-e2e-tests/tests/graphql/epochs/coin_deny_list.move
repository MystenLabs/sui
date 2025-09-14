// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# advance-epoch

//# publish --sender A
#[allow(deprecated_usage)]
module P::coin {
  use sui::coin::{Self, CoinMetadata, DenyCapV2, TreasuryCap};
  use sui::deny_list::DenyList;

  public struct COIN() has drop;

  public struct Bundle has key, store {
    id: UID,
    treasury: TreasuryCap<COIN>,
    deny: DenyCapV2<COIN>,
    metadata: CoinMetadata<COIN>,
  }

  fun init(otw: COIN, ctx: &mut TxContext) {
    let (treasury, deny, metadata) = coin::create_regulated_currency_v2(
      otw,
      9,
      b"COIN",
      b"Coin",
      b"A test coin",
      option::none(),
      true,
      ctx,
    );

    transfer::public_share_object(Bundle {
      id: object::new(ctx),
      treasury,
      deny,
      metadata,
    });
  }

  public fun poke_deny_list(
    deny_list: &mut DenyList,
    bundle: &mut Bundle,
    ctx: &mut TxContext,
  ) {
    coin::deny_list_v2_add(deny_list, &mut bundle.deny, @0x1234, ctx)
  }
}

//# programmable --sender A --inputs object(0x403) object(2,0)
//> P::coin::poke_deny_list(Input(0), Input(1))

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# run-graphql
{
  e0: epoch(epochId: 0) {
    coinDenyList { address version }
  }

  e1: epoch(epochId: 1) {
    coinDenyList { address version }
  }

  e2: epoch(epochId: 2) {
    coinDenyList { address version }
  }
}
