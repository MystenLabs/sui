// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator --addresses P=0x0

// When a nested dynamic field is modified, its root object is also modified,
// but intermediate dynamic fields in the path are not, so it is not always
// safe to bound a dynamic field by the version of its immediate parent.

//# publish
module P::M {
  use sui::object_bag::ObjectBag;

  public struct C has key, store {
    id: UID,
    c: u64
  }

  public fun nest(outer: &mut ObjectBag, mut inner: ObjectBag, ctx: &mut TxContext) {
    let counter = C {
      id: object::new(ctx),
      c: 0,
    };

    inner.add(2u64, counter);
    outer.add(1u64, inner);
  }

  public fun poke(outer: &mut ObjectBag) {
    let inner = outer.borrow_mut<_, ObjectBag>(1u64);
    let counter = inner.borrow_mut<_, C>(2u64);
    counter.c = counter.c + 1;
  }
}

//# programmable --sender A --inputs @A
//> 0: sui::object_bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs @A
//> 0: sui::object_bag::new();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(2,0) object(3,0)
//> 0: P::M::nest(Input(0), Input(1))

//# programmable --sender A --inputs object(2,0)
//> 0: P::M::poke(Input(0))

//# create-checkpoint

//# run-graphql --cursors bcs(1u64) bcs(2u64)
{
  objectVersions(address: "@{obj_2_0}") {
    nodes {
      version
      asMoveObject {
        dynamicObjectField(name: { type: "u64", bcs: "@{cursor_0}" }) {
          version
          name { json }
          value {
            ... on MoveObject {
              version
              dynamicObjectField(name: { type: "u64", bcs: "@{cursor_1}" }) {
                version
                name { json }
                value {
                  ... on MoveObject {
                    version
                    contents { json }
                  }
                }
              }
            }
          }
        }
      }
    }
  }
}
