// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --simulator

//# run-graphql
{
    object(address: "0x2") {
        receivedTransactionBlocks {
            nodes {
                digest
            }
        }
    }
}

//# publish
module P0::m {
    public struct Obj has key {
        id: UID
    }

    fun init(ctx: &mut TxContext) {
        let obj = Obj { id: object::new(ctx) };
        transfer::transfer(obj, @0x2)
    }
}

//# create-checkpoint

//# run-graphql
{
    object(address: "0x2") {
        receivedTransactionBlocks {
            nodes {
                digest
            }
        }
    }
}
