// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 --simulator

//# publish

module P0::m {
    public fun bang() {
        abort 42
    }
}

//# programmable
//> P0::m::bang()

//# create-checkpoint

//# run-graphql

# The last transaction should have failed, what's its error message?
{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                status
                errors
            }
        }
    }
}
