// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --simulator

//# advance-clock --duration-ns 123000000

//# advance-epoch

//# advance-clock --duration-ns 321000000

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# run-graphql --cursors 0 5
{
    all: epochs(first: 10) { ...E }
    first: epochs(first: 3) { ...E }
    last: epochs(last: 3) { ...E }
    firstBefore: epochs(first: 3, before: "@{cursor_1}") { ...E }
    lastAfter: epochs(last: 3, after: "@{cursor_0}") { ...E }
    firstAfter: epochs(first: 3, after: "@{cursor_0}") { ...E }
    lastBefore: epochs(last: 3, before: "@{cursor_1}") { ...E }
    afterBefore: epochs(after: "@{cursor_0}", before: "@{cursor_1}") { ...E }
}

fragment E on EpochConnection {
    pageInfo {
        hasPreviousPage
        hasNextPage
    }
    nodes {
        epochId
    }
}