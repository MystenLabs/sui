// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses P1=0x0 P2=0x0 --simulator

//# publish --sender A
module P1::M1 {
    use sui::event;

    public struct T1 has copy, drop {}
    public struct T2 has copy, drop {}
    public struct T3 has copy, drop {}

    public fun new_T3(): T3 {
        T3 {}
    }

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

//# publish --sender B --dependencies P1
module P2::M2 {
    use sui::event;
    use P1::M1::{T3};

    public struct T4 has copy, drop {}

    public struct EventC<T> has copy, drop {
        value: T
    }
    
    public fun emit_T3() {
        // Use a public constructor function from P1::M1
        let t3_instance = P1::M1::new_T3();
        event::emit(EventC<T3> { value: t3_instance })
    }

    public fun emit_T4() {
        // Use a public constructor function from P1::M1
        event::emit(EventC<T4> { value: T4 {} })
    }
}

//# create-checkpoint

//# run P1::M1::emit_T1 --sender A

//# run P1::M1::emit_T2 --sender A

//# run P1::M1::emit_both --sender A

//# run P1::M1::emit_both --sender B

//# run P2::M2::emit_T3 --sender B

//# run P2::M2::emit_T4 --sender B

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
  # This should only match EventC whose type is defined in P1
  eventsOfTypeP1EmittedByP2: events(first: 50, filter: {type: "@{P2}::M2::EventC<@{P1}::M1::T3>"}) {
    nodes {
      ...E
    }
  }
}

fragment E on Event {
  sequenceNumber
  transactionModule { package { address } }
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