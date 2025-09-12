// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --addresses P=0x0

//# publish
module P::M {
  use sui::dynamic_field as df;
  use sui::dynamic_object_field as dof;

  public struct O has key, store {
    id: UID,
    x: u64,
  }

  public fun o(x: u64, ctx: &mut TxContext): O {
    O { id: object::new(ctx), x }
  }

  public fun df(p: &mut O, k: u64, c: O) {
    df::add(&mut p.id, k, c);
  }

  public fun dof(p: &mut O, k: u64, c: O) {
    dof::add(&mut p.id, k, c);
  }
}

//# create-checkpoint

// Create a tree of objects:
//
// O(3)
// |- 20 -> O(4) - 30 -> O(5)
// |= 20 => O(6) - 40 -> O(7)
// `- 50 -> O(8)

//# programmable --sender A --inputs 3u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 4u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 5u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 6u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 7u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs 8u64 @A
//> 0: P::M::o(Input(0));
//> 1: TransferObjects([Result(0)], Input(1));

//# programmable --sender A --inputs object(3,0) object(4,0) object(5,0) object(6,0) object(7,0) object(8,0) 20u64 30u64 40u64 50u64
//> 0: P::M::df(Input(1), Input(7), Input(2));
//> 1: P::M::df(Input(0), Input(6), Input(1));
//> 2: P::M::df(Input(3), Input(8), Input(4));
//> 3: P::M::dof(Input(0), Input(6), Input(3));
//> 4: P::M::df(Input(0), Input(9), Input(5));

//# create-checkpoint

//# run-graphql
{ # Paginate dynamic fields at the latest version via `address` and `object`
  address(address: "@{obj_3_0}") {
    dynamicFields {
      nodes {
        name { json }
        value {
          ... on MoveValue { json }
          ... on MoveObject {
            contents { json }
            dynamicFields {
              nodes {
                name { json }
                value { ... on MoveValue { json } }
              }
            }
          }
        }
      }
    }
  }

  object(address: "@{obj_3_0}") {
    version
    asMoveObject {
      dynamicFields {
        nodes {
          name { json }
          value {
            ... on MoveValue { json }
            ... on MoveObject {
              contents { json }
              dynamicFields {
                nodes {
                  name { json }
                  value { ... on MoveValue { json } }
                }
              }
            }
          }
        }
      }
    }
  }

  # Fetching dynamic fields on a wrapped object
  wrapped: address(address: "@{obj_4_0}") {
    dynamicFields {
      nodes {
        name { json }
        value { ... on MoveValue { json } }
      }
    }
  }
}

//# run-graphql
{ # It's an error to paginate dynamic fields with a version set
  address(address: "@{obj_4_0}", rootVersion: 8) {
    dynamicFields {
      nodes {
        name { json }
        value { ... on MoveValue { json } }
      }
    }
  }
}

//# run-graphql
{ # Another situation that sets a version
  object(address: "@{obj_3_0}", version: 8) {
    asMoveObject {
      dynamicFields {
        nodes {
          name { json }
          value { ... on MoveValue { json } }
        }
      }
    }
  }
}
