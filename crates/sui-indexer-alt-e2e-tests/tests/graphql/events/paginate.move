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

// Transaction that emits a single event
//# run P1::M1::emit_event --sender A --args 42

// Transaction that emits multiple events
//# run P1::M1::emit_multiple_events --sender A

// Transaction that emits multiple events
//# run P1::M1::emit_events_by_count --sender A --args 10

// Transaction with no events (transfer)
//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{
  allEvents: events(first: 50) {
    pageInfo { ...pageInfo }
    nodes {
      ...E
    }
  }
  eventsHasNext: events(first: 2) {
    pageInfo { ...pageInfo }
    nodes {
      ...E
    }
  }
  eventsHasPrevious: events(last: 2) {
    pageInfo { ...pageInfo }
    nodes {
      ...E
    }
  }
}

fragment pageInfo on PageInfo {
  startCursor
  endCursor
  hasNextPage
  hasPreviousPage
}

fragment E on Event {
  sequenceNumber
  timestamp
  transaction {
    digest
  }
}

//# run-graphql --cursors {"t":2,"e":0}
{
  eventsAfterT2E0HasPreviousPage: events(first: 50, after: "@{cursor_0}") {
    pageInfo { 
      hasNextPage
      hasPreviousPage 
    }
  }
}

//# run-graphql --cursors {"t":4,"e":9}
{
  eventsBeforeT3E9HasNextPage: events(last: 50, before: "@{cursor_0}") {
    pageInfo { 
      hasNextPage
      hasPreviousPage 
    }
  }
}

//# run-graphql --cursors {"t":2,"e":0} {"t":4,"e":9}
{
  firstEventBetweenT2E0AndT4E9: events(first: 1, after: "@{cursor_0}", before: "@{cursor_1}") {
    nodes {
      ...E
    }
  }
  lastEventBetweenT2E0AndT4E9: events(last: 1, after: "@{cursor_0}", before: "@{cursor_1}") {
    nodes {
      ...E
    }
  }
  eventsBetweenInvalidCursors: events(first: 10, after: "@{cursor_1}", before: "@{cursor_0}") {
    nodes {
      ...E
    }
  }
}

fragment E on Event {
  sequenceNumber
  timestamp
  transaction {
    digest
  }
}


//# run-graphql --cursors {"t":2,"e":0}
{
  eventsAfterTransaction2AndEvent0: events(first: 1, after: "@{cursor_0}") {
    pageInfo { 
      startCursor
      endCursor
      hasNextPage
      hasPreviousPage 
    }
    nodes {
      sequenceNumber
      timestamp
      transaction { digest }
    }
  }
}

//# run-graphql --cursors {"t":3,"e":0}
{
  eventsBeforeTransaction3AndEvent0: events(first: 1, before: "@{cursor_0}") {
    pageInfo { 
      startCursor
      endCursor
      hasNextPage
      hasPreviousPage 
    }
    nodes {
      sequenceNumber
      timestamp
      transaction { digest }
    }
  }
}

//# run-graphql --cursors {"t":3,"e":1}
{
  eventsBeforeTransaction3AndEvent1FromBack: events(last: 2, before: "@{cursor_0}") {
    pageInfo { 
      startCursor
      endCursor
      hasNextPage
      hasPreviousPage 
    }
    nodes {
      sequenceNumber
      timestamp
      transaction { digest }
    }
  }
}