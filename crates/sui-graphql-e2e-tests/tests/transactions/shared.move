// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --simulator

//# publish
module P0::m {
    public struct Foo has key {
        id: UID,
        x: u64,
    }

    fun init(ctx: &mut TxContext) {
        transfer::share_object(Foo {
            id: object::new(ctx),
            x: 0,
        })
    }

    public fun get(f: &Foo): u64 { f.x }
    public fun inc(f: &mut Foo) { f.x = f.x + 1 }
}

//# programmable --inputs immshared(1,0)
//> 0: P0::m::get(Input(0))

//# programmable --inputs object(1,0)
//> 0: P0::m::inc(Input(0))

//# programmable --inputs object(1,0)
//> 0: P0::m::get(Input(0));
//> P0::m::inc(Input(0))

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(last: 3) {
    nodes {
      kind {
        __typename
        ... on ProgrammableTransactionBlock {
          transactions {
            nodes {
              ... on MoveCallTransaction {
                package
                module
                functionName
              }
            }
          }
        }
      }
      effects {
        status
        unchangedSharedObjects {
          nodes {
            __typename
            ... on SharedObjectRead {
              address
              version
              digest
              object {
                asMoveObject {
                  contents {
                    type {
                      repr
                    }
                    json
                  }
                }
              }
            }
            ... on SharedObjectDelete {
              address
              version
              mutable
            }
          }
        }
      }
    }
  }
}
