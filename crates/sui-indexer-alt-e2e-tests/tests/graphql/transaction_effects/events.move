// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 --simulator

//# publish
module test::events_test {
    use std::ascii;
    use sui::event;

    public struct TestEvent has copy, drop {
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

        event::emit(TestEvent {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }
}

// Transaction that emits a single event
//# run test::events_test::emit_event --sender A --args 42

// Transaction that emits multiple events
//# run test::events_test::emit_multiple_events --sender A

// Transaction with no events (transfer)
//# programmable --sender A --inputs 100 @B
//> 0: SplitCoins(Gas, [Input(0)]);
//> 1: TransferObjects([Result(0)], Input(1))

//# create-checkpoint

//# run-graphql
{ # Test events field on transaction with single event
  singleEventTransaction: transactionEffects(digest: "@{digest_2}") {
    events {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        sender {
          address
        }
        sequenceNumber
        timestamp
        eventBcs
        contents {
          type { repr }
          json
        }
        transaction {
          digest
        }
        transactionModule {
          package { address }
          name
        }
      }
    }
  }
}

//# run-graphql
{ # Test events field on transaction with multiple events
  multipleEventsTransaction: transactionEffects(digest: "@{digest_3}") {
    events {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        sender {
          address
        }
        sequenceNumber
        timestamp
        eventBcs
        contents {
          type { repr }
          json
        }
        transaction {
          digest
        }
        transactionModule {
          package { address }
          name
        }
      }
    }
  }
}

//# run-graphql
{ # Test pagination functionality with multiple events
  paginationTest: transactionEffects(digest: "@{digest_3}") {
    events(first: 1) {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        sender {
          address
        }
        sequenceNumber
        timestamp
        eventBcs
        contents {
          type { repr }
          json
        }
        transaction {
          digest
        }
        transactionModule {
          package { address }
          name
        }
      }
    }
  }
}

//# run-graphql
{ # Test backward pagination functionality with with multiple events
  backwardPaginationTest: transactionEffects(digest: "@{digest_3}") {
    events(last: 1) {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        sender {
          address
        }
        sequenceNumber
        timestamp
        eventBcs
        contents {
          type { repr }
          json
        }
        transaction {
          digest
        }
        transactionModule {
          package { address }
          name
        }
      }
    }
  }
}

//# run-graphql
{ # Test events field on transaction with no events
  noEventsTransaction: transactionEffects(digest: "@{digest_4}") {
    events {
      pageInfo {
        hasNextPage
        hasPreviousPage
      }
      nodes {
        sender {
          address
        }
        sequenceNumber
        timestamp
        eventBcs
        contents {
          type { repr }
          json
        }
        transaction {
          digest
        }
        transactionModule {
          package { address }
          name
        }
      }
    }
  }
}
