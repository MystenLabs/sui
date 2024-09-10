// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

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

    public entry fun sum(left: &Object, right: &Object): u64 {
        left.value + right.value
    }

    public entry fun increment(object: &mut Object, value: u64) {
        object.value = object.value + value;
    }
}

//# run Test::M1::create --args 2 @A

//# run Test::M1::create --args 3 @A

//# run Test::M1::create --args 4 @A

//# run Test::M1::create --args 5 @A

//# run Test::M1::create --args 6 @A

//# programmable --sender A --inputs object(2,0) object(3,0) object(4,0) object(5,0) object(6,0) @A
//> 0: Test::M1::sum(Input(0), Input(1));
//> 1: Test::M1::sum(Input(2), Input(3));
//> 2: Test::M1::sum(Input(0), Input(4));
//> 3: Test::M1::create(Result(2), Input(5));

//# run Test::M1::increment --sender A --args object(7,0) 100

//# create-checkpoint


//# run-graphql
{
  transactionBlocks(last: 1) {
    nodes {
      digest
      effects {
        dependencies {
          pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
          }
          edges {
            cursor
            node {
              digest
              kind {
                __typename
                ... on ProgrammableTransactionBlock {
                  transactions {
                    nodes {
                      ... on MoveCallTransaction {
                        module
                        functionName
                      }
                    }
                  }
                }
              }
            }
            cursor
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"i":0,"c":1}
{
  transactionBlocks(last: 1) {
    nodes {
      digest
      effects {
        dependencies(first: 1, after: "@{cursor_0}") {
          pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
          }
          edges {
            cursor
            node {
              digest
              kind {
                __typename
                ... on ProgrammableTransactionBlock {
                  transactions {
                    nodes {
                      ... on MoveCallTransaction {
                        module
                        functionName
                      }
                    }
                  }
                }
              }
            }
            cursor
          }
        }
      }
    }
  }
}
