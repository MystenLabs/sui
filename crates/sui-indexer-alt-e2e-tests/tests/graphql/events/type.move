// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish --sender A
module test::events_test {
    use sui::event;
    use std::ascii;

    public struct TestEvent has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public struct TestEvent2 has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public entry fun emit_event(value: u64) {
        event::emit(TestEvent {
            message: ascii::string(b"Hello from test event"),
            value,
        });
    }

    public entry fun emit_multiple_events() {
        event::emit(TestEvent {
            message: ascii::string(b"First event"),
            value: 1,
        });

        event::emit(TestEvent2 {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }

    public entry fun emit_events_by_count(count: u64) {
        let mut i = 0;
        while (i < count) {
            event::emit(TestEvent {
                message: ascii::string(b"Event from loop"),
                value: i + 1,
            });
            i = i + 1;
        };
    }
}
//# create-checkpoint

// Transaction that emits a single event
//# run test::events_test::emit_event --sender A --args 42

//# create-checkpoint

// Transaction that emits a single event
//# run test::events_test::emit_event --sender A --args 42

// Transaction that emits multiple events
//# run test::events_test::emit_multiple_events --sender A

//# create-checkpoint

// Transaction that emits multiple events
//# run test::events_test::emit_multiple_events --sender A

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
  eventsOfTypeTestEvent: events(first: 50, filter: {type: "0xc1cce56ea94b622b2e115e9393f270ed68c856803cbadf7e954a5bebffc0857e::events_test::TestEvent"}) {
    nodes {
      ...E
    }
  }
  eventsOfTypeTestEvent2: events(first: 50, filter: {type: "0xc1cce56ea94b622b2e115e9393f270ed68c856803cbadf7e954a5bebffc0857e::events_test::TestEvent2"}) {
    nodes {
      ...E
    }
  }
}

fragment E on Event {
  sequenceNumber
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