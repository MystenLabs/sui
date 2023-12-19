// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::event;

    struct EventA has copy, drop {
        new_value: u64
    }

    public entry fun emit_1(value: u64) {
        event::emit(EventA { new_value: value })
    }

    public entry fun emit_2(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1})
    }
}

//# run Test::M1::emit_1 --sender A --args 0

//# run Test::M1::emit_2 --sender A --args 1

//# create-checkpoint

//# run-graphql
{
  transactionBlockConnection(filter: {sentAddress: "@{A}"}) {
    nodes {
      events {
        edges {
          node {
            sendingModule {
              name
            }
            type {
              repr
            }
            senders {
              address
            }
            json
            bcs
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(filter: {sentAddress: "@{A}"}) {
    nodes {
      events(first: 2 after: "2:0", filter: {sender: "@{A}"}) {
        edges {
          node {
            sendingModule {
              name
            }
            type {
              repr
            }
            senders {
              address
            }
            json
            bcs
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlockConnection(filter: {sentAddress: "@{A}"}) {
    nodes {
      events(last: 2 before: "3:1", filter: {sender: "@{A}"}) {
        edges {
          node {
            sendingModule {
              name
            }
            type {
              repr
            }
            senders {
              address
            }
            json
            bcs
          }
        }
      }
    }
  }
}
