// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P1=0x0 P2=0x0 --simulator

//# publish --sender A
module P1::M1 {
    use sui::event;
    use std::ascii;

    public struct EventA has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public struct EventB has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public entry fun emit_multiple_events() {
        event::emit(EventA {
            message: ascii::string(b"First event"),
            value: 1,
        });

        event::emit(EventB {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }
}

//# publish --sender B
module P2::M2 {
    use sui::event;
    use std::ascii;

    public struct EventC has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public struct EventD has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public entry fun emit_multiple_events() {
        event::emit(EventC {
            message: ascii::string(b"First event"),
            value: 1,
        });

        event::emit(EventD {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }
}
//# create-checkpoint

//# run P1::M1::emit_multiple_events --sender B

//# run P2::M2::emit_multiple_events --sender B

//# run P2::M2::emit_multiple_events --sender A

//# create-checkpoint

//# run-graphql
{
  allEvents: events(first: 50) {
    nodes {
      ...E
    }
  }
  eventsSentByA: events(first: 50, filter: {sender: "@{A}"}) {
    nodes {
      ...E
    }
  }
  eventsSentByB: events(first: 50, filter: {sender: "@{B}"}) {
    nodes {
      ...E
    }
  }
  eventsOfP1: events(first: 50, filter: {module: "@{P1}"}) {
    nodes {
      ...E
    }
  }
  eventOfP1M1: events(first: 50, filter: {module: "@{P1}::M1"}) {
    nodes {
        ...E
    }
  }
  eventsOfP2: events(first: 50, filter: {module: "@{P2}"}) {
    nodes {
      ...E
    }
  }
  # This should match events of type P1::M1::EventA (filtering by event type module)
  eventsByTypeModuleP1M1: events(first: 50, filter: {type: "@{P1}::M1"}) {
    nodes {
      ...E
    }
  }
  # This should match events of type P2::M2::EventB (filtering by event type module)
  eventsByTypeModuleP2M2: events(first: 50, filter: {type: "@{P2}::M2"}) {
    nodes {
      ...E
    }
  }
  eventsOfP2SentByB: events(first: 50, filter: {sender: "@{B}", module: "@{P2}"}) {
    nodes {
      ...E
    }
  }
  eventsOfP1SentByAShouldBeEmpty: events(first: 50, filter: {sender: "@{A}", module: "@{P1}"}) {
    nodes {
      ...E
    }
  }
}

fragment E on Event {
  sequenceNumber
  sender { address }
  contents { type { repr } }
  transaction {
    digest,
    effects {
        checkpoint {
            sequenceNumber
        }
    }
  }
}

//# run-graphql
{
  eventsByModuleAndTypeIsUnavailable: events(first: 50, filter: {module: "@{P1}", type: "@{P1}::M1::EventC"}) {
    nodes {
      sequenceNumber
    }
  }
}