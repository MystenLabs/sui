// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

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
    use sui::event;
    use Test::M1;

    public struct EventB has copy, drop {
        new_value: u64
    }

    public fun emit_emit_a(value: u64) {
        M1::emit_a(value);
    }

    public fun emit_b(value: u64) {
        event::emit(EventB { new_value: value })
    }
}

//# run Test::M2::emit_emit_a --sender A --args 20

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1::EventA"}) {
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

//# run Test::M2::emit_b --sender A --args 42

//# run Test::M2::emit_b --sender B --args 43

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1"}) {
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

//# run-graphql
{
  events(filter: {sender: "@{A}", eventType: "@{Test}::M2"}) {
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

//# run-graphql
{
  events(filter: {sender: "@{A}", eventType: "@{Test}"}) {
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

//# run-graphql
{
  events(filter: {eventType: "@{Test}"}) {
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

