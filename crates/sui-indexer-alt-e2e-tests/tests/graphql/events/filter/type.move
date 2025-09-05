// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P1=0x0 --simulator

//# publish --sender A
module P1::M1 {
    use sui::event;

    public struct T1 has copy, drop {}
    public struct T2 has copy, drop {}

    public struct EventA<T> has copy, drop {
        value: T
    }

    public fun emit_T1() {
        event::emit(EventA<T1> { value: T1 {} })
    }

    public fun emit_T2() {
        event::emit(EventA<T2> { value: T2 {} })
    }

    public fun emit_both() {
        event::emit(EventA<T1> { value: T1 {} });
        event::emit(EventA<T2> { value: T2 {} })
    }
}

//# create-checkpoint

//# run P1::M1::emit_T1 --sender A

//# run P1::M1::emit_T2 --sender A

//# run P1::M1::emit_both --sender A

//# run P1::M1::emit_both --sender B

//# create-checkpoint

//# run-graphql
{
  allEvents: events(first: 50) {
    nodes {
      ...E
    }
  }
  eventsOfP1M1EventA: events(first: 50, filter: {type: "@{P1}::M1::EventA"}) {
    nodes {
      ...E
    }
  }
  eventsOfP1M1EventATypeT2: events(first: 50, filter: {type: "@{P1}::M1::EventA<@{P1}::M1::T2>"}) {
    nodes {
      ...E
    }
  }
  eventsOfP1M1EventABySenderB: events(first: 50, filter: {type: "@{P1}::M1::EventA", sender: "@{B}"}) {
    nodes {
      ...E
    }
  }
  eventsOfP1M1EventAByDigest: events(first: 50, filter: {type: "@{P1}::M1::EventA", transactionDigest: "@{digest_6}"}) {
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