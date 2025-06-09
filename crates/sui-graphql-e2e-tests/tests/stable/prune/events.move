// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Test that we handle pruned events. When paginating on a cursor, it should be bounded by
// checkpoint consistency. Last graphql query checks that newer events exist.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator --epochs-to-keep 1

//# publish
module Test::M1 {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public struct EventB<phantom T> has copy, drop {
        new_value: u64
    }

    public struct Object has key, store {
        id: UID,
        value: u64,
    }

    public entry fun create(value: u64, recipient: address, ctx: &mut TxContext) {
        transfer::public_transfer(
            Object { id: object::new(ctx), value },
            recipient
        )
    }

    public entry fun emit_a(o1: &mut Object, value: u64) {
        o1.value = value;
        event::emit(EventA { new_value: value })
    }
}

//# run Test::M1::create --sender A --args 0 @A

//# run Test::M1::emit_a --sender A --args object(2,0) 0

//# create-checkpoint

//# advance-epoch

//# run Test::M1::emit_a --sender A --args object(2,0) 1

//# create-checkpoint

//# run-graphql --wait-for-checkpoint-pruned 1
{
  events(filter: {sender: "@{A}"}) {
    edges {
      cursor
      node {
        transactionBlock {
          effects {
            checkpoint {
              sequenceNumber
            }
          }
        }
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}

//# create-checkpoint

//# run Test::M1::emit_a --sender A --args object(2,0) 2

//# run Test::M1::emit_a --sender A --args object(2,0) 3

//# create-checkpoint

//# run-graphql --cursors {"tx":5,"e":0,"c":3}
{
  events(filter: {sender: "@{A}"}, after: "@{cursor_0}") {
    edges {
      cursor
      node {
        transactionBlock {
          effects {
            checkpoint {
              sequenceNumber
            }
          }
        }
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}

//# run-graphql
{
  events(filter: {sender: "@{A}"}) {
    edges {
      cursor
      node {
        transactionBlock {
          effects {
            checkpoint {
              sequenceNumber
            }
          }
        }
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}
