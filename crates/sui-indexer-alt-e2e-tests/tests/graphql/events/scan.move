// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P1=0x0 P2=0x0 --accounts A B C --simulator

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

    public struct GenericEvent<phantom T> has copy, drop {
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

    public entry fun emit_generic_bool(value: u64) {
        event::emit(GenericEvent<bool> { value });
    }

    public entry fun emit_generic_u64(value: u64) {
        event::emit(GenericEvent<u64> { value });
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

// Checkpoint 5: Multiple events in one checkpoint (A emits 3 EventA)
//# run P1::M1::emit_multiple --sender A --args 3

//# create-checkpoint

// Checkpoint 6: A emits GenericEvent<bool>
//# run P1::M1::emit_generic_bool --sender A --args 10

//# create-checkpoint

// Checkpoint 7: B emits GenericEvent<u64>
//# run P1::M1::emit_generic_u64 --sender B --args 20

//# create-checkpoint

//# advance-epoch

// Generate 10,501 checkpoints to create 10 complete blocks + 500 extra for incomplete block
//# create-checkpoint 10501

// Checkpoint after blocks: B emits EventC from P2
//# run P2::M2::emit_c --sender B --args 100

//# create-checkpoint

//# run-graphql
# Scan vs indexed cross-check, multi-filter combinations, empty-result edge cases,
# checkpoint-bounded scan, and scan across bloom blocks.
{
  scanEventsA: scanEvents(filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }
  paginateEventsA: events(filter: { sender: "@{A}" }) { ...EV }

  eventsAFromP1: scanEvents(filter: {
    sender: "@{A}",
    module: "@{P1}",
    beforeCheckpoint: 10512
  }) { ...EV }
  scanTripleFilter: scanEvents(filter: {
    sender: "@{A}",
    module: "@{P1}",
    type: "@{P1}::M1::EventA",
    beforeCheckpoint: 10512
  }) { ...EV }

  emptyNonExistent: scanEvents(filter: {
    sender: "0x0000000000000000000000000000000000000000000000000000000000000001",
    beforeCheckpoint: 10512
  }) { ...EV }

  earlyEvents: scanEvents(filter: {
    sender: "@{A}",
    beforeCheckpoint: 4
  }) { ...EV }
  atCheckpoint5: scanEvents(filter: {
    sender: "@{A}",
    atCheckpoint: 5
  }) { ...EV }

  noFilter: scanEvents(first: 5, filter: {
    afterCheckpoint: 0,
    beforeCheckpoint: 6
  }) { ...EV }

  scanByTypeOnly: scanEvents(filter: {
    type: "@{P2}::M2::EventC",
    beforeCheckpoint: 6
  }) { ...EV }

  scanAcrossBlocks: scanEvents(filter: {
    sender: "@{A}",
    afterCheckpoint: 0,
    beforeCheckpoint: 10512
  }) { ...EV }

  # Primitive type param filtering: GenericEvent<bool> vs GenericEvent<u64>
  scanGenericBool: scanEvents(filter: {
    type: "@{P1}::M1::GenericEvent<bool>",
    beforeCheckpoint: 10512
  }) { ...EV }
  scanGenericU64: scanEvents(filter: {
    type: "@{P1}::M1::GenericEvent<u64>",
    beforeCheckpoint: 10512
  }) { ...EV }
  # Without type params, should return both GenericEvent<bool> and GenericEvent<u64>
  scanGenericAny: scanEvents(filter: {
    type: "@{P1}::M1::GenericEvent",
    beforeCheckpoint: 10512
  }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}

//# run-graphql --cursors {"c":1,"t":3,"e":0} {"c":5,"t":7,"e":0} {"c":5,"t":7,"e":2} {"c":20,"t":20,"e":0}
# Cursor pagination: cursor_0 is cp1 event (t=3,e=0),
# cursor_1 is cp5 first event (t=7,e=0), cursor_2 is cp5 last event (t=7,e=2).
{
  first2: scanEvents(first: 2, filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }
  last2: scanEvents(last: 2, filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }

  firstAfter: scanEvents(first: 10, after: "@{cursor_0}", filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }
  firstBefore: scanEvents(first: 10, before: "@{cursor_2}", filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }

  windowFirst: scanEvents(first: 10, after: "@{cursor_0}", before: "@{cursor_2}", filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }

  nonexistentCursor: scanEvents(last: 10, after: "@{cursor_3}", filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }
  invalidOrder: scanEvents(first: 10, after: "@{cursor_2}", before: "@{cursor_0}", filter: { sender: "@{A}", beforeCheckpoint: 10512 }) { ...EV }
}

fragment EV on EventConnection {
  pageInfo { startCursor endCursor hasPreviousPage hasNextPage }
  edges { cursor node { sequenceNumber sender { address } contents { type { repr } } } }
}
