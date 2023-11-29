// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# publish --upgradeable --sender A

module P0::m0 { public fun f(): u64 { P0::n::f() } }
module P0::m1 { public fun f(): u64 { P0::n::f() } }
module P0::m2 { public fun f(): u64 { P0::n::f() } }

module P0::n {
    friend P0::m0;
    friend P0::m1;
    friend P0::m2;
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
                nodes { name }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # After is an exclusive lower bound, so only expect two modules in
            # the page, and an indication there's a previous page.
            after: friendConnection(after: "0") {
                nodes { name }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Before is an exclusive upper bound, so only expect two modules in
            # the page, and an indication there's a next page.
            before: friendConnection(before: "2") {
                nodes { name }
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
                nodes { name }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
            prefixAll: friendConnection(after: "0", first: 2) {
                nodes { name }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Limit the number of elements in the page using `last` and skip
            # elements from the end using `before`.
            suffix: friendConnection(before: "2", last: 1) {
                nodes { name }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
            suffixAll: friendConnection(before: "2", last: 2) {
                nodes { name }
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

//# upgrade --package P0 --upgrade-capability 1,1 --sender A

module P0::m0 { public fun f(): u64 { P0::n::f() } }
module P0::m1 { public fun f(): u64 { P0::n::f() } }
module P0::m2 { public fun f(): u64 { P0::n::f() } }
module P0::m3 { public fun f(): u64 { P0::n::f() } }

module P0::n {
    friend P0::m0;
    friend P0::m1;
    friend P0::m2;
    friend P0::m3;

    public fun f(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql

# Get the names of all friend modules in the upgraded package.  One of
# the modules (m3) is new in the upgraded package, so for this query
# to work properly, the module needs to be aware of its storage ID,
# and not its runtime ID.
fragment ModuleFriends on Object {
    asMovePackage {
        module(name: "n") {
            friendConnection {
                nodes { name }
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
