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
            # Fetch the names of all friend modules and check that there are no
            # pages on either side.
            all: friendConnection {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # After is an exclusive lower bound, so only expect two modules in
            # the page, and an indication there's a previous page.
            after: friendConnection(after: "0") {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Before is an exclusive upper bound, so only expect two modules in
            # the page, and an indication there's a next page.
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
            # Limit the number of elements in the page using `first` and skip
            # elements using `after`.
            prefix: friendConnection(after: "0", first: 1) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
            prefixAll: friendConnection(after: "0", first: 2) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Limit the number of elements in the page using `last` and skip
            # elements from the end using `before`.
            suffix: friendConnection(before: "2", last: 1) {
                nodes { moduleId { name } }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
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
