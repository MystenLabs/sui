// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses pkg=0x0 --simulator

//# publish

module pkg::m {
    use sui::coin::{Self, Coin};
    use sui::sui::SUI;

    public fun foo<C: drop>(x: u64, c: &Coin<C>): u64 {
        coin::value(c) + x
    }

    public fun bar(c: &Coin<SUI>): u64 {
        foo(42, c) * foo(43, c)
    }
}

module pkg::n {
    public fun baz(): u32 {
        44
    }
}

//# create-checkpoint

//# run-graphql

fragment Modules on Object {
    location
    asMovePackage {
        module(name: "m") {
            moduleId {
                name
                package {
                    asObject {
                        location
                    }
                }
            }
            fileFormatVersion
            bytes
            disassembly
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...Modules
                    }
                }
            }
        }
    }
}
