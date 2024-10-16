// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Tests that fetching events filtered on both emitting module and event would result
// in an error.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public fun emit_a(value: u64) {
        event::emit(EventA { new_value: value })
    }
}

module Test::M2 {
    use Test::M1;

    public fun emit_emit_a(value: u64) {
        M1::emit_a(value);
    }
}

//# run Test::M2::emit_emit_a --sender A --args 20

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}", emittingModule: "@{Test}::M2", eventType: "@{Test}::M1::EventA"}) {
    nodes {
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
        bcs
        json
      }
    }
  }
}
