// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses pkg=0x0 --simulator

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
    address
    asMovePackage {
        module(name: "m") {
            name
            package { address }

            fileFormatVersion
            bytes
            disassembly
        }
    }
}

{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                objectChanges {
                    nodes {
                        outputState {
                            ...Modules
                        }
                    }
                }
            }
        }
    }
}

//# run-graphql --cursors {"n":"m","c":1} {"n":"o","c":1}
fragment NodeNames on MoveModuleConnection {
    edges {
        cursor
        node { name }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

fragment Modules on Object {
    address
    asMovePackage {
        # Tests to make sure `after` and `before` correctly limit the
        # upper and lower bounds on the range of modules, and
        # correctly detect the existence of predecessor or successor
        # pages.

        all: modules { ...NodeNames }
        after: modules(after: "@{cursor_0}") { ...NodeNames }
        before: modules(before: "@{cursor_1}") { ...NodeNames }
    }
}

{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                objectChanges {
                    nodes {
                        outputState {
                            ...Modules
                        }
                    }
                }
            }
        }
    }
}

//# run-graphql --cursors {"n":"m","c":1} {"n":"o","c":1}
fragment NodeNames on MoveModuleConnection {
    edges {
        cursor
        node { name }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

fragment Modules on Object {
    address
    asMovePackage {
        # Tests to make sure `first` and `last` correctly limit the
        # number of modules returned and correctly detect the
        # existence of predecessor or successor pages.

        prefix: modules(after: "@{cursor_0}", first: 1) { ...NodeNames }
        prefixAll: modules(after: "@{cursor_0}", first: 2) { ...NodeNames }
        prefixExcess: modules(after: "@{cursor_0}", first: 20) { ...NodeNames }

        suffix: modules(before: "@{cursor_1}", last: 1) { ...NodeNames }
        suffixAll: modules(before: "@{cursor_1}", last: 2) { ...NodeNames }
        suffixExcess: modules(before: "@{cursor_1}", last: 20) { ...NodeNames }
    }
}

{
    transactionBlocks(last: 1) {
        nodes {
            effects {
                objectChanges {
                    nodes {
                        outputState {
                            ...Modules
                        }
                    }
                }
            }
        }
    }
}
