// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Creates an event Test::M1::EventA, Test::M1::EventB<Object>, Test::M2::EventA, Test::M2::EventB<Object>
// Verifies all 4 events show up when filtered on sender
// Verifies all 4 events show up when filtered on sender and package
// Verifies 2 events show up when filtered on sender, package and module
// Verifies correct event when filtered for Test::M1::EventA
// Verifies correct event when filtered for Test::M1::EventB
// Verifies error when filtered on sender, package, module and event type with generics and <

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

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

    public entry fun emit_b(o1: &mut Object, value: u64) {
        o1.value = value;
        event::emit<EventB<Object>>(EventB { new_value: value })
    }
}

module Test::M2 {
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
        event::emit(EventA { new_value: value });
    }

    public entry fun emit_b(o1: &mut Object, value: u64) {
        o1.value = value;
        event::emit<EventB<Object>>(EventB { new_value: value })
    }
}

//# run Test::M1::create --sender A --args 0 @A

//# run Test::M1::emit_a --sender A --args object(2,0) 0

//# run Test::M1::emit_b --sender A --args object(2,0) 1

//# run Test::M2::create --sender A --args 2 @A

//# run Test::M2::emit_a --sender A --args object(5,0) 2

//# run Test::M2::emit_b --sender A --args object(5,0) 3

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1::EventA"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1::EventB"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}::M1::EventB<"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "::M1"}) {
    edges {
      cursor
      node {
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
  events(filter: {sender: "@{A}", eventType: "@{Test}::"}) {
    edges {
      cursor
      node {
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
