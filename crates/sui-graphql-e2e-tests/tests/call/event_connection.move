// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
    use sui::event;
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;
    use sui::coin::Coin;

    struct EventA has copy, drop {
        new_value: u64
    }

    struct EventB<phantom T> has copy, drop {
        new_value: u64
    }

    struct Object has key, store {
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
    use sui::object::{Self, UID};
    use sui::tx_context::TxContext;
    use sui::transfer;
    use sui::coin::Coin;

    struct EventA has copy, drop {
        new_value: u64
    }

    struct EventB<phantom T> has copy, drop {
        new_value: u64
    }

    struct Object has key, store {
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

//# run-graphql --variables A
{
  eventConnection(
    filter: {sender: $A}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}

//# run-graphql --variables A Test
{
  eventConnection(
    filter: {sender: $A, eventPackage: $Test}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}

//# run-graphql --variables A Test
{
  eventConnection(
    filter: {sender: $A, eventPackage: $Test, eventModule: "M1"}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}

//# run-graphql --variables A Test
{
  eventConnection(
    filter: {sender: $A, eventPackage: $Test, eventModule: "M1", eventType: "EventA"}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}

//# run-graphql --variables A Test
{
  eventConnection(
    filter: {sender: $A, eventPackage: $Test, eventModule: "M1", eventType: "EventB"}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}

//# run-graphql --variables A Test
{
  eventConnection(
    filter: {sender: $A, eventPackage: $Test, eventModule: "M1", eventType: "EventB<"}
  ) {
    nodes {
      sendingModule {
        name
      }
      type {
        repr
      }
      senders {
        location
      }
      json
      bcs
    }
  }
}
