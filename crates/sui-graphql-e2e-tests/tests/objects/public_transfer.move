// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish
module P0::m {
    public struct Foo has key, store {
        id: UID,
    }

    public struct Bar has key {
        id: UID,
    }

    public fun foo(ctx: &mut TxContext): Foo {
        Foo { id: object::new(ctx) }
    }

    public fun bar(ctx: &mut TxContext) {
        transfer::transfer(
            Bar { id: object::new(ctx) },
            tx_context::sender(ctx),
        )
    }
}

//# programmable --inputs @A
//> 0: P0::m::foo();
//> 1: P0::m::bar();
//> TransferObjects([Result(0)], Input(0))

//# create-checkpoint

//# run-graphql
{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                objectChanges {
                    nodes {
                        outputState {
                            asMoveObject {
                                contents { type { repr } }
                                hasPublicTransfer
                            }
                        }
                    }
                }
            }
        }
    }
}
