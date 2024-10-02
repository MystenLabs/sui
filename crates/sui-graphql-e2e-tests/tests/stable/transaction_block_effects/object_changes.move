// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 A=0x42 --simulator --custom-validator-account --reference-gas-price 234 --default-gas-price 1000

//# publish
module Test::M1 {
    use sui::coin::Coin;

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    fun foo<T: key, T2: drop>(_p1: u64, value1: T, _value2: &Coin<T2>, _p2: u64): T {
        value1
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }
}

//# run Test::M1::create --args 0 @A --gas-price 1000

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(first: 1) {
    nodes {
      effects {
        objectChanges {
          pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
          }
          edges {
            cursor
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"i":10,"c":1}
{
  transactionBlocks(first: 1) {
    nodes {
      effects {
        objectChanges(first: 5, after: "@{cursor_0}") {
          pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
          }
          edges {
            cursor
          }
        }
      }
    }
  }
}
