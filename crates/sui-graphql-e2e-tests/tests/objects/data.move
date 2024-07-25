// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish
module P0::m {
    use std::ascii::{Self, String as ASCII};
    use std::string::{Self, String as UTF8};

    public struct Foo has key, store {
        id: UID,
        f0: ID,
        f1: bool,
        f2: u8,
        f3: u64,
        f4: UTF8,
        f5: ASCII,
        f6: vector<address>,
        f7: Option<u32>,
    }

    public struct Bar has key {
        id: UID,
    }

    public fun foo(ctx: &mut TxContext): Foo {
        let id = object::new(ctx);
        let f0 = object::uid_to_inner(&id);
        let f1 = true;
        let f2 = 42;
        let f3 = 43;
        let f4 = string::utf8(b"hello");
        let f5 = ascii::string(b"world");
        let f6 = vector[object::uid_to_address(&id)];
        let f7 = option::some(44);
        Foo { id, f0, f1, f2, f3, f4, f5, f6, f7 }
    }
}

//# programmable --inputs @A
//> 0: P0::m::foo();
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
                                contents {
                                    type { repr }
                                    data
                                    json
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
