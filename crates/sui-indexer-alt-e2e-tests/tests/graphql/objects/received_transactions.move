// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --addresses P0=0x0 --accounts A B --simulator

//# publish
module P0::m {
    public struct Obj has key {
        id: UID,
        value: u64,
    }

    fun init(ctx: &mut TxContext) {
        let obj = Obj { id: object::new(ctx), value: 0 };
        transfer::transfer(obj, @0x2)
    }

    public fun create_and_transfer(value: u64, recipient: address, ctx: &mut TxContext) {
        let obj = Obj { id: object::new(ctx), value };
        transfer::transfer(obj, recipient)
    }
}

//# create-checkpoint

// A transfers an object to 0x2 (checkpoint 2)
//# run P0::m::create_and_transfer --sender A --args 100 @0x2

//# create-checkpoint

// B transfers an object to 0x2 (checkpoint 3)  
//# run P0::m::create_and_transfer --sender B --args 200 @0x2

//# create-checkpoint

//# run-graphql
{
    object(address: "0x2") {
        # All received transactions
        allReceived: receivedTransactions {
            edges {
                node {
                    digest
                    sender { address }
                    effects { checkpoint { sequenceNumber } }
                }
            }
        }
        # Filter by checkpoint
        fromCheckpoint2: receivedTransactions(filter: { atCheckpoint: 2 }) {
            edges {
                node {
                    digest
                    sender { address }
                }
            }
        }
        # Filter by sender A
        fromA: receivedTransactions(filter: { sentAddress: "@{A}" }) {
            edges {
                node {
                    digest
                    sender { address }
                }
            }
        }
    }
}
