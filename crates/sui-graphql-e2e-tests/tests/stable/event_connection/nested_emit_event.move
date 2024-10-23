// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// This test demonstrates that one can search for events emitted by a package or module.
// The emitting module is where the entrypoint function is defined -
// in other words, the function called by a programmable transaction block.

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

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

    public fun yeet(value: u64) {
        M1::emit_a(value);
    }
}

module Test::M3 {
  use Test::M2;

  public entry fun yeet(value: u64) {
    M2::yeet(value);
  }
}

//# run Test::M3::yeet --sender A --args 2

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}"}) {
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
  events(filter: {sender: "@{A}", emittingModule: "@{Test}"}) {
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
  events(filter: {sender: "@{A}", emittingModule: "@{Test}::M1"}) {
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
  events(filter: {sender: "@{A}", emittingModule: "@{Test}::M2"}) {
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
  events(filter: {sender: "@{A}", emittingModule: "@{Test}::M3"}) {
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
