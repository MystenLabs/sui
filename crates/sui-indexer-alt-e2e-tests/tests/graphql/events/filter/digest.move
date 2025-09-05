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

        event::emit(EventA {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }

}
//# create-checkpoint

// Transaction that emits a single event
//# run P1::M1::emit_event --sender A --args 42

//# create-checkpoint

// Transaction that emits a single event
//# run P1::M1::emit_multiple_events --sender A

//# create-checkpoint

// Transaction that emits multiple events
//# run P1::M1::emit_event --sender B --args 42

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
  eventByDigest3: events(first: 50, filter: {digest: "@{digest_3}"}) {
    nodes {
      ...E
    }
  }
  eventsByDigest5: events(first: 50, filter: {digest: "@{digest_5}"}) {
    nodes {
      ...E
    }
  }
  eventsByDigest5AtCheckpoint6OutsideOfRange: events(first: 50, filter: {digest: "@{digest_5}", atCheckpoint: 6}) {
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

//# run-graphql --cursors {"t":3,"e":0}
{
  eventByDigestAfterT3E0HasPreviousPage: events(first: 50, after: "@{cursor_0}", filter: {digest: "@{digest_5}"}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
    nodes {
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
  }
}

//# run-graphql --cursors {"t":3,"e":1}
{
  eventByDigestBeforeT3E1HasNextPage: events(first: 50, before: "@{cursor_0}", filter: {digest: "@{digest_5}"}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
    }
    nodes {
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
  }
}


//# run-graphql --cursors {"t":10,"e":0}
{
  eventByDigestCursorsOutsideRangeIsEmpty: events(last: 50, before: "@{cursor_0}", filter: {digest: "@{digest_5}"}) {
    pageInfo {
      hasNextPage
      hasPreviousPage
      startCursor
      endCursor
    }
    nodes {
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
  }
}