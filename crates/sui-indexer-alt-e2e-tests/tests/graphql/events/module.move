// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A B --addresses test=0x0 test2=0x0 --simulator

//# publish --sender A
module test::module_test {
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
}

//# publish --sender B
module test2::module_test2 {
    use sui::event;
    use std::ascii;

    public struct TestEvent3 has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public struct TestEvent4 has copy, drop {
        message: ascii::String,
        value: u64,
    }

    public entry fun emit_multiple_events() {
        event::emit(TestEvent3 {
            message: ascii::string(b"First event"),
            value: 1,
        });

        event::emit(TestEvent4 {
            message: ascii::string(b"Second event"),
            value: 2,
        });
    }
}
//# create-checkpoint

//# run test::module_test::emit_multiple_events --sender B

//# run test2::module_test2::emit_multiple_events --sender B

//# run test2::module_test2::emit_multiple_events --sender A

//# create-checkpoint

//# run-graphql
{
  allEvents: events(first: 50) {
    nodes {
      ...E
    }
  }
  eventsSentBySenderA: events(first: 50, filter: {sender: "@{A}"}) {
    nodes {
      ...E
    }
  }
  eventsSentBySenderB: events(first: 50, filter: {sender: "@{B}"}) {
    nodes {
      ...E
    }
  }
  eventsOfTestModuleByPackageId: events(first: 50, filter: {module: "@{test}"}) {
    nodes {
      ...E
    }
  }
  eventsOfTestModuleByPackageAndModule: events(first: 50, filter: {type: "@{test}::module_test"}) {
    nodes {
        ...E
    }
  }
  eventsOfTestModule2ByPackageId: events(first: 50, filter: {module: "@{test2}"}) {
    nodes {
      ...E
    }
  }
  eventsOfTestModule2ByPackageAndModule: events(first: 50, filter: {type: "@{test2}::module_test2"}) {
    nodes {
        ...E
    }
  }
  eventsSentBySenderBAndPackageId: events(first: 50, filter: {sender: "@{B}", module: "@{test2}"}) {
    nodes {
      ...E
    }
  }
  eventsSentBySenderAAndPackageIdShouldBeEmpty: events(first: 50, filter: {sender: "@{A}", module: "@{test}"}) {
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
  eventsByModuleAndTypeIsUnavailable: events(first: 50, filter: {module: "@{test}", type: "@{test}::module_test::TestEvent3"}) {
    nodes {
      sequenceNumber
    }
  }
}