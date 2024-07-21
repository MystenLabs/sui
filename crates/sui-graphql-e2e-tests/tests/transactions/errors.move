// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --simulator

//# publish

module P0::m {
    public fun tick(): u64 { 42 }
    public fun boom() { abort 42 }
}

//# programmable
//> P0::m::boom()

//# create-checkpoint

//# run-graphql

# The last transaction should have failed, what's its error message?
{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                status
                errors
            }
        }
    }
}

//# programmable
//> 0: P0::m::tick();
//> 1: P0::m::tick();
//> P0::m::boom()

//# create-checkpoint

//# run-graphql

# Check the transaction command ordinal is correct if the abort
# happens at a later command.
{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                status
                errors
            }
        }
    }
}
