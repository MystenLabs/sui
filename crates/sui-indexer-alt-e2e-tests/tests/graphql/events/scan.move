// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B C --addresses P1=0x0 P2=0x0 --simulator

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

    public entry fun emit_a(value: u64) {
        event::emit(EventA {
            message: ascii::string(b"Event A"),
            value,
        });
    }

    public entry fun emit_b(value: u64) {
        event::emit(EventB {
            message: ascii::string(b"Event B"),
            value,
        });
    }

    public entry fun emit_multiple(count: u64) {
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

//# publish
module P2::M2 {
    use sui::event;
    use std::ascii;

    public struct EventC has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public entry fun emit_c(value: u64) {
        event::emit(EventC {
            message: ascii::string(b"Event C from P2"),
            value,
        });
    }
}

// Checkpoint 1: A emits EventA from P1::M1
//# run P1::M1::emit_a --sender A --args 1

//# create-checkpoint

// Checkpoint 2: B emits EventB from P1::M1
//# run P1::M1::emit_b --sender B --args 2

//# create-checkpoint

// Checkpoint 3: A emits EventC from P2::M2
//# run P2::M2::emit_c --sender A --args 3

//# create-checkpoint

// Checkpoint 4: C emits EventA from P1::M1
//# run P1::M1::emit_a --sender C --args 4

//# create-checkpoint

// Checkpoint 5: Multiple events in one checkpoint (A emits 3 events)
//# run P1::M1::emit_multiple --sender A --args 3

//# create-checkpoint

//# advance-epoch

// Generate checkpoints to create bloom filter blocks
//# create-checkpoint 10501

// Checkpoint after blocks: B emits EventC from P2
//# run P2::M2::emit_c --sender B --args 100

//# create-checkpoint

//# run-graphql
# Test basic eventsScan with sender filter
{
  eventsFromA: eventsScan(filter: { sender: "@{A}" }) { ...EV }
  eventsFromB: eventsScan(filter: { sender: "@{B}" }) { ...EV }
  eventsFromC: eventsScan(filter: { sender: "@{C}" }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with module filter (package only)
{
  eventsFromP1: eventsScan(filter: { module: "@{P1}" }) { ...EV }
  eventsFromP2: eventsScan(filter: { module: "@{P2}" }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with type filter
{
  eventTypeA: eventsScan(filter: { type: "@{P1}::M1::EventA" }) { ...EV }
  eventTypeB: eventsScan(filter: { type: "@{P1}::M1::EventB" }) { ...EV }
  eventTypeC: eventsScan(filter: { type: "@{P2}::M2::EventC" }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with multi-filter (sender + module)
{
  # A's events from P1
  eventsAFromP1: eventsScan(filter: {
    sender: "@{A}",
    module: "@{P1}"
  }) { ...EV }

  # A's events from P2
  eventsAFromP2: eventsScan(filter: {
    sender: "@{A}",
    module: "@{P2}"
  }) { ...EV }

  # B's events from P1
  eventsBFromP1: eventsScan(filter: {
    sender: "@{B}",
    module: "@{P1}"
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with multi-filter (sender + type)
{
  # A's EventA events
  eventsATypeA: eventsScan(filter: {
    sender: "@{A}",
    type: "@{P1}::M1::EventA"
  }) { ...EV }

  # C's EventA events
  eventsCTypeA: eventsScan(filter: {
    sender: "@{C}",
    type: "@{P1}::M1::EventA"
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with triple filter (sender + module + type) - this is allowed in scan but not in events
{
  eventsAP1TypeA: eventsScan(filter: {
    sender: "@{A}",
    module: "@{P1}",
    type: "@{P1}::M1::EventA"
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with checkpoint bounds
{
  # Events in checkpoints 1-3
  earlyEvents: eventsScan(filter: {
    sender: "@{A}",
    beforeCheckpoint: 4
  }) { ...EV }

  # Events after checkpoint 3
  laterEvents: eventsScan(filter: {
    sender: "@{A}",
    afterCheckpoint: 3
  }) { ...EV }

  # Events at specific checkpoint
  atCheckpoint5: eventsScan(filter: {
    sender: "@{A}",
    atCheckpoint: 5
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql
# Test eventsScan with empty results
{
  # Non-existent sender
  emptyNonExistent: eventsScan(filter: {
    sender: "0x0000000000000000000000000000000000000000000000000000000000000001"
  }) { ...EV }

  # Conflicting filters - C never used P2
  emptyConflicting: eventsScan(filter: {
    sender: "@{C}",
    module: "@{P2}"
  }) { ...EV }

  # Beyond data range
  emptyBeyondData: eventsScan(filter: {
    sender: "@{A}",
    afterCheckpoint: 50000
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo {
    startCursor
    endCursor
    hasPreviousPage
    hasNextPage
  }
  edges {
    cursor
    node {
      sequenceNumber
      sender { address }
      transactionModule { package { address } name }
      contents { type { repr } }
    }
  }
}

//# run-graphql --cursors {"t":2,"e":0,"c":1} {"t":0,"e":0,"c":3} {"t":0,"e":0,"c":5} {"t":0,"e":2,"c":5}
# Test eventsScan pagination
# A's events: cp1/t2/e0, cp3/t0/e0, cp5/t0/e0, cp5/t0/e1, cp5/t0/e2
{
  # Basic pagination
  first2: eventsScan(first: 2, filter: { sender: "@{A}" }) { ...EV }
  last2: eventsScan(last: 2, filter: { sender: "@{A}" }) { ...EV }

  # After first event (cp1/t2/e0) - should get remaining 4 events
  afterFirst: eventsScan(first: 10, after: "@{cursor_0}", filter: { sender: "@{A}" }) { ...EV }

  # After second event (cp3/t0/e0) - should get 3 events from cp5
  afterSecond: eventsScan(first: 10, after: "@{cursor_1}", filter: { sender: "@{A}" }) { ...EV }

  # Before last event (cp5/t0/e2) - should get first 4 events
  beforeLast: eventsScan(last: 10, before: "@{cursor_3}", filter: { sender: "@{A}" }) { ...EV }

  # Bounded range: after cp1 event, before cp5/e2 - should get cp3 and cp5/e0, cp5/e1
  betweenCursors: eventsScan(first: 10, after: "@{cursor_0}", before: "@{cursor_3}", filter: { sender: "@{A}" }) { ...EV }

  # Invalid cursor order (after > before) - should return empty
  invalidOrder: eventsScan(first: 10, after: "@{cursor_2}", before: "@{cursor_1}", filter: { sender: "@{A}" }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}

//# run-graphql
# Test scanning across block boundaries
{
  scanAcrossBlocks: eventsScan(filter: {
    sender: "@{A}",
    afterCheckpoint: 0,
    beforeCheckpoint: 10510
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}
