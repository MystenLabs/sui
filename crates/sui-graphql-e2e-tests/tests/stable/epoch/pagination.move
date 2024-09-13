// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --simulator --accounts C

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch


// TODO: Short term hack to get around indexer epoch issue
//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

//# advance-epoch

//# create-checkpoint

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