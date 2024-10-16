// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public entry fun emit_1(value: u64) {
        event::emit(EventA { new_value: value })
    }

    public entry fun emit_2(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1})
    }

        public entry fun emit_3(value: u64) {
        event::emit(EventA { new_value: value });
        event::emit(EventA { new_value: value + 1});
        event::emit(EventA { new_value: value + 2});
    }
}

//# run Test::M1::emit_1 --sender A --args 1

//# run Test::M1::emit_2 --sender A --args 10

//# run Test::M1::emit_3 --sender A --args 100

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(filter: { sentAddress: "@{A}" }) {
    nodes {
      effects{
        events {
          edges {
            node {
              sendingModule {
                name
              }
              contents {
                json
                bcs
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  events(filter: { sender: "@{A}" }) {
    nodes {
      sendingModule {
        name
      }
      contents {
        json
        bcs
      }
    }
  }
}

//# run-graphql
{
  transactionBlocks(first: 1, filter: { sentAddress: "@{A}" }) {
    nodes {
      effects {
        events(last: 1) {
          edges {
            node {
              sendingModule {
                name
              }
              contents {
                json
                bcs
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql --cursors {"i":0,"c":1}
{
  transactionBlocks(last: 1, filter: { sentAddress: "@{A}" }) {
    nodes {
      effects {
        events(first: 2, after: "@{cursor_0}") {
          edges {
            node {
              sendingModule {
                name
              }
              contents {
                json
                bcs
              }
            }
          }
        }
      }
    }
  }
}

//# run-graphql
{
  transactionBlocks {
    nodes {
      digest
      effects {
        events {
          nodes {
            transactionBlock {
              digest
            }
          }
        }
      }
    }
  }
}
