// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 106 --addresses Test=0x0 --accounts A B C --simulator --enable-accumulators

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

//# run Test::M1::create --sender A --args 0 @A

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --sender A --args 1 @B

//# create-checkpoint

//# advance-epoch

//# run Test::M1::create --sender C --args 2 @A

//# create-checkpoint

// Send 1000 address balance from A to B and A to C
//# programmable --sender A --inputs 1000 @B @C
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: sui::coin::into_balance<sui::sui::SUI>(Result(0));
//> 2: sui::balance::send_funds<sui::sui::SUI>(Result(1), Input(1));
//> 3: SplitCoins(Gas, [Input(0)]);
//> 4: sui::coin::into_balance<sui::sui::SUI>(Result(3));
//> 5: sui::balance::send_funds<sui::sui::SUI>(Result(4), Input(2));

//# create-checkpoint

//# run-graphql
{
  transactionsA: transactions(filter: { affectedAddress: "@{A}" }) { ...TX }
  transactionsA_sentByC: transactions(filter: { affectedAddress: "@{A}", sentAddress: "@{C}" }) { ...TX }
  transactionsB: transactions(filter: { affectedAddress: "@{B}" }) { ...TX }
  transactionsC: transactions(filter: { affectedAddress: "@{C}" }) { ...TX }
}

fragment TX on TransactionConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      digest
      effects {
        checkpoint {
          sequenceNumber
          digest
        }
      }
    }
  }
}