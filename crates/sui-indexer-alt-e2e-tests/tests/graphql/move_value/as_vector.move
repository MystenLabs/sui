// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 108 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  public struct Foo has key, store {
    id: UID,
    values: vector<u64>,
    nested: vector<vector<u64>>,
    addr: address,
  }

  public fun create(ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      values: vector[11, 22, 33, 44],
      nested: vector[
        vector[1, 2],
        vector[3],
        vector[],
      ],
      addr: ctx.sender(),
    }
  }
}

//# programmable --sender A --inputs @A
//> 0: test::mod::create();
//> 1: TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{ # Paginate over a vector and recursively inspect nested vector elements.
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        values: extract(path: "values") {
          asVector(first: 2) {
            pageInfo {
              hasPreviousPage
              hasNextPage
            }
            nodes {
              type { repr }
              json
            }
          }
        }

        nested: extract(path: "nested") {
          asVector(first: 2) {
            pageInfo {
              hasPreviousPage
              hasNextPage
            }
            nodes {
              json
              asVector {
                pageInfo {
                  hasPreviousPage
                  hasNextPage
                }
                nodes {
                  json
                }
              }
            }
          }
        }

        notVector: extract(path: "addr") {
          asVector {
            nodes { json }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors 1
{ # Continue pagination from the second element.
  object(address: "@{obj_2_0}") {
    asMoveObject {
      contents {
        extract(path: "values") {
          asVector(after: "@{cursor_0}", first: 2) {
            pageInfo {
              hasPreviousPage
              hasNextPage
            }
            nodes {
              json
            }
          }
        }
      }
    }
  }
}
