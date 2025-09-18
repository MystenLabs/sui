// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P1=0x0 --simulator

//# publish
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

    public entry fun emit_event(value: u64) {
        event::emit(EventA {
            message: ascii::string(b"Hello from test event"),
            value,
        });
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

    public entry fun emit_events_by_count(count: u64) {
        let mut i = 0;
        while (i < count) {
            event::emit(EventA {
                message: ascii::string(b"Event from loop"),
                value: i + 1,
            });
            i = i + 1;
        };
    }
}
//# create-checkpoint

// Transaction that emits a single event
//# run P1::M1::emit_event --sender A --args 42

//# create-checkpoint

// Transaction that emits a single event
//# run P1::M1::emit_event --sender A --args 42

// Transaction that emits multiple events
//# run P1::M1::emit_multiple_events --sender A

//# create-checkpoint

// Transaction that emits multiple events
//# run P1::M1::emit_multiple_events --sender A

// Transaction with no events (transfer)
//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  allEvents: events(first: 50) {
    nodes {
      ...E
    }
  }
  eventsAfterCheckpoint3: events(first: 50, filter: {afterCheckpoint: 3}) {
    nodes {
      ...E
    }
  }
  eventsBeforeCheckpoint4: events(last: 50, filter: {beforeCheckpoint: 4}) {
    nodes {
      ...E
    }
  }
  eventsAtCheckpoint4: events(last: 50, filter: {atCheckpoint: 4}) {
    nodes {
      ...E
    }
  }
  # Should be the same as allEvents since there are no events at Cp0
  eventsAfterCheckpoint0: events(first: 50, filter: {afterCheckpoint: 0}) {
    nodes {
      ...E
    }
  }
  eventsBeforeCheckpoint100: events(last: 50, filter: {beforeCheckpoint: 100}) {
    nodes {
      ...E
    }
  }
  eventsAtCheckpoint100NonExistent: events(last: 50, filter: {atCheckpoint: 100}) {
    nodes {
      ...E
    }
  }
}

fragment E on Event {
  sequenceNumber
  transaction {
    digest,
    effects {
        checkpoint {
            sequenceNumber
        }
    }
  }
}