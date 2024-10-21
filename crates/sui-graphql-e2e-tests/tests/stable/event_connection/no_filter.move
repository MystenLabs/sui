// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses Test=0x0 --accounts A --simulator

//# publish
module Test::M1 {
    use sui::event;

    public struct EventA has copy, drop {
        new_value: u64
    }

    public entry fun no_emit(value: u64): u64 {
        value
    }

    public entry fun emit(value: u64) {
        let mut i = 0;
        while (i < 51) {
            event::emit(EventA { new_value: value + i });
            i = i + 1;
        }
    }
}

//# run Test::M1::emit --sender A --args 0

//# create-checkpoint

//# run-graphql
{
    events {
        pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
        }
        nodes {
          contents {
            json
          }
        }
    }
}

//# run-graphql --cursors {"tx":2,"e":19,"c":1}
{
    events(after: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
            startCursor
            endCursor
        }
        nodes {
            contents {
                json
            }
        }
    }
}
