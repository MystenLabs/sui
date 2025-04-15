// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# advance-epoch

//# run-graphql 
{
    epochs(last:2) {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql 
{
    epochs(first:3) {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":5,"e":2} 
{
    epochs(before: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":3,"e":4} 
{
    epochs(before: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":11,"e":1}
{
    epochs(after: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":0,"e":5}
{
    epochs(before: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":3,"e":4}
{
    epochs(after: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}

//# run-graphql --cursors {"c":0,"e":0}
{
    epochs(after: "@{cursor_0}") {
        pageInfo {
            hasPreviousPage
            hasNextPage
        }
        nodes {
            epochId
        }
    }
}
