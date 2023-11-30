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

module pkg::o {
    public fun qux(): u32 {
        45
    }
}

//# create-checkpoint

//# run-graphql

fragment Modules on Object {
    location
    asMovePackage {
        module(name: "m") {
            name
            package { asObject { location } }

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

//# run-graphql
fragment NodeNames on MoveModuleConnection {
    nodes { name }
    pageInfo { hasNextPage hasPreviousPage }
}

fragment Modules on Object {
    location
    asMovePackage {
        # Tests to make sure `after` and `before` correctly limit the
        # upper and lower bounds on the range of modules, and
        # correctly detect the existence of predecessor or successor
        # pages.

        all: moduleConnection { ...NodeNames }
        after: moduleConnection(after: "m") { ...NodeNames }
        before: moduleConnection(before: "o") { ...NodeNames }
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

//# run-graphql
fragment NodeNames on MoveModuleConnection {
    nodes { name }
    pageInfo { hasNextPage hasPreviousPage }
}

fragment Modules on Object {
    location
    asMovePackage {
        # Tests to make sure `first` and `last` correctly limit the
        # number of modules returned and correctly detect the
        # existence of predecessor or successor pages.

        prefix: moduleConnection(after: "m", first: 1) { ...NodeNames }
        prefixAll: moduleConnection(after: "m", first: 2) { ...NodeNames }
        prefixExcess: moduleConnection(after: "m", first: 100) { ...NodeNames }

        suffix: moduleConnection(before: "o", last: 1) { ...NodeNames }
        suffixAll: moduleConnection(before: "o", last: 2) { ...NodeNames }
        suffixExcess: moduleConnection(before: "o", last: 100) { ...NodeNames }
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
