// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A B --simulator

//# publish
module Test::M1 {
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


//# run Test::M1::emit_T1 --sender A

//# run Test::M1::emit_T2 --sender A

//# run Test::M1::emit_both --sender A

//# create-checkpoint

//# run-graphql
{
    transactionBlocks {
        nodes {
            digest
        }
    }
}

//# run-graphql
{
  events(filter: {eventType: "@{Test}::M1::EventA"}) {
    nodes {
      sender {
        address
      }
      contents {
        type {
          repr
        }
        json
      }
    }
  }
}

//# run-graphql
{
  events(filter: {eventType: "@{Test}::M1::EventA<@{Test}::M1::T1>"}) {
    nodes {
      sender {
        address
      }
      contents {
        type {
          repr
        }
        json
      }
    }
  }
}

//# run-graphql
{
  events(filter: {eventType: "@{Test}::M1::EventA<@{Test}::M1::T2>", transactionDigest: "9nu1ivpL9hHcbJ9GwGfmD3Kuet5w74t2GBp8f1Ggy3UD"}) {
    nodes {
      sender {
        address
      }
      contents {
        type {
          repr
        }
        json
      }
    }
  }
}
