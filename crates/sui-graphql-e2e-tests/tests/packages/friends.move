// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses pkg=0x0 --simulator

//# publish

module pkg::m0 { public fun f(): u64 { pkg::n::f() } }
module pkg::m1 { public fun f(): u64 { pkg::n::f() } }
module pkg::m2 { public fun f(): u64 { pkg::n::f() } }

module pkg::n {
    friend pkg::m0;
    friend pkg::m1;
    friend pkg::m2;
    public fun f(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql

fragment ModuleFriends on Object {
    asMovePackage {
        module(name: "n") {
            all: friendConnection {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            after: friendConnection(after: "0") {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            before: friendConnection(before: "2") {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...ModuleFriends
                    }
                }
            }
        }
    }
}


//# run-graphql

fragment ModuleFriends on Object {
    asMovePackage {
        module(name: "n") {
            prefix: friendConnection(after: "0", first: 1) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            prefixAll: friendConnection(after: "0", first: 2) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            suffix: friendConnection(before: "2", last: 1) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            suffixAll: friendConnection(before: "2", last: 2) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...ModuleFriends
                    }
                }
            }
        }
    }
}
