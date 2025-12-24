// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses test=0x0 --simulator

//# publish --sender A
module test::mod {
  use sui::bag::{Self, Bag};

  public struct Foo has key, store {
    id: UID,
    creator: address,
    cap: ID,
    bag: Option<Bag>,
  }

  public struct FooCap has key, store {
    id: UID,
  }

  public fun cap(ctx: &mut TxContext): FooCap {
    FooCap {
      id: object::new(ctx),
    }
  }

  public fun foo(cap: &FooCap, ctx: &mut TxContext): Foo {
    Foo {
      id: object::new(ctx),
      creator: ctx.sender(),
      cap: object::id(cap),
      bag: option::none(),
    }
  }

  public fun bag(foo: &mut Foo, ctx: &mut TxContext) {
    foo.bag.fill(bag::new(ctx))
  }

  public fun add(foo: &mut Foo, key: u64, val: u64) {
    foo.bag.borrow_mut().add(key, val)
  }

  public fun set(foo: &mut Foo, key: u64, val: u64) {
    *(&mut foo.bag.borrow_mut()[key]) = val;
  }
}

//# create-checkpoint

//# programmable --sender A --inputs @A
//> 0: test::mod::cap();
//> 1: TransferObjects([Result(0)], Input(0))

//# programmable --sender A --inputs object(3,0) @A
//> 0: test::mod::foo(Input(0));
//> 1: TransferObjects([Result(0)], Input(1))

//# programmable --sender A --inputs object(4,0) @A
//> 0: test::mod::bag(Input(0))

//# create-checkpoint

//# programmable --sender A --inputs object(4,0) 1u64 200u64
//> 0: test::mod::add(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(4,0) 3u64 400u64
//> 0: test::mod::add(Input(0), Input(1), Input(2))

//# create-checkpoint

//# programmable --sender A --inputs object(4,0) 3u64 4u64
//> 0: test::mod::set(Input(0), Input(1), Input(2))

//# programmable --sender A --inputs object(4,0) 1u64 2u64
//> 0: test::mod::set(Input(0), Input(1), Input(2))

//# create-checkpoint

//# run-graphql
{ # Extract an address and fetch its owned objects
  object(address: "@{obj_4_0}") {
    asMoveObject {
      contents {
        extract(path: "creator") {
          asAddress {
            objects {
              nodes {
                contents {
                  type { repr }
                  json
                }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Extract an address and then fetch its transactions and their object changes.
  object(address: "@{obj_4_0}") {
    asMoveObject {
      contents {
        extract(path: "creator") {
          asAddress {
            transactions {
              nodes {
                effects {
                  objectChanges {
                    nodes {
                      idCreated
                      inputState { ...O }
                      outputState { ...O }
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
}

fragment O on Object {
  asMoveObject {
    contents {
      type { repr }
      json
    }
  }
}

//# run-graphql
{ # Fetch another object being pointed to.
  object(address: "@{obj_4_0}") {
    asMoveObject {
      contents {
        extract(path: "cap") {
          asAddress {
            asObject {
              asMoveObject {
                contents {
                  type { repr }
                  json
                }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(1u64)
{ # Fetch a wrapped object and request one of its dynamic fields, two ways.
  object(address: "@{obj_4_0}") {
    asMoveObject {
      contents {
        field: extract(path: "bag->[1u64]") {
          json
        }

        bag: extract(path: "bag.id") {
          asAddress {
            dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
              value { ... on MoveValue { json } }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors bcs(3u64)
{ # Fetch the object at multiple versions and fetch one of its dynamic fields, two ways.
  objectVersions(address: "@{obj_4_0}") {
    nodes {
      asMoveObject {
        contents {
          field: extract(path: "bag->[3u64]") {
            json
          }

          bag: extract(path: "bag.id") {
            asAddress {
              dynamicField(name: { type: "u64", bcs: "@{cursor_0}" }) {
                value { ... on MoveValue { json } }
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{ # Fetch a wrapped object and request all its dynamic fields at one checkpoint
  # before, and the current checkpoint.
  #
  # If the value is a UID, this requires either re-scoping it at a checkpoint,
  # or unwrapping it to get the ID inside to make the nested owned object query.
  object(address: "@{obj_4_0}") {
    asMoveObject {
      contents {
        uid: extract(path: "bag.id") {
          asAddress {
            before: addressAt(checkpoint: 4) { ...DF }
            latest: addressAt { ...DF }
          }
        }

        id: extract(path: "bag.id.id") {
          asAddress { ...DF }
        }
      }
    }
  }
}

fragment DF on Address {
  dynamicFields {
    nodes {
      name { ...MV }
      value { ...MV }
    }
  }
}

fragment MV on MoveValue { json }
