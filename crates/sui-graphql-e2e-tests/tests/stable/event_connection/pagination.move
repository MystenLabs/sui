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
}

//# run Test::M1::emit_1 --sender A --args 0

//# run Test::M1::emit_2 --sender A --args 1

//# create-checkpoint

//# run-graphql
{
  events(filter: {sender: "@{A}"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}

//# run-graphql --cursors {"tx":2,"e":0,"c":1}
{
  events(first: 2 after: "@{cursor_0}", filter: {sender: "@{A}"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}

//# run-graphql --cursors {"tx":3,"e":1,"c":1}
{
  events(last: 2 before: "@{cursor_0}", filter: {sender: "@{A}"}) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        sendingModule {
          name
        }
        sender {
          address
        }
        contents {
          type {
            repr
          }
          json
          bcs
        }

      }
    }
  }
}

//# run-graphql
{
  events(last: 2) {
    pageInfo {
      hasPreviousPage
      hasNextPage
    }
    edges {
      cursor
      node {
        sendingModule {
          name
        }
        sender {
          address
        }        
        contents {
          type {
            repr
          }
          json
          bcs
        }
      }
    }
  }
}
