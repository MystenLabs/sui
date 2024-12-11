// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish --sender A
module P0::fake {
    use sui::coin;

    public struct FAKE has drop {}

    fun init(witness: FAKE, ctx: &mut TxContext){
        let (mut treasury_cap, metadata) = coin::create_currency(
            witness,
            2,
            b"FAKE",
            b"",
            b"",
            option::none(),
            ctx,
        );

        let c1 = coin::mint(&mut treasury_cap, 1, ctx);
        let c2 = coin::mint(&mut treasury_cap, 2, ctx);
        let c3 = coin::mint(&mut treasury_cap, 3, ctx);

        transfer::public_freeze_object(metadata);
        transfer::public_transfer(treasury_cap, tx_context::sender(ctx));
        transfer::public_transfer(c1, tx_context::sender(ctx));
        transfer::public_transfer(c2, tx_context::sender(ctx));
        transfer::public_transfer(c3, tx_context::sender(ctx));
    }
}

//# create-checkpoint

//# run-graphql
fragment C on Coin {
  coinBalance
  contents { type { repr } }
}

{
  suiCoins: coins {
    edges {
      cursor
      node { ...C }
    }
  }

  fakeCoins: coins(type: "@{P0}::fake::FAKE") {
    edges {
      cursor
      node { ...C }
    }
  }

  address(address: "@{A}") {
    coins {
      edges {
        cursor
        node { ...C }
      }
    }

    allBalances: balances {
      edges {
        cursor
        node {
          coinType { repr }
          coinObjectCount
          totalBalance
        }
      }
    }

    firstBalance: balances(first: 1) {
      edges { cursor }
    }

    lastBalance: balances(last: 1) {
      edges { cursor }
    }
  }
}
