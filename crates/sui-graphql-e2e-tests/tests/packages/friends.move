// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# publish --upgradeable --sender A

module P0::m0 { public fun f(): u64 { P0::n::f() } }
module P0::m1 { public fun f(): u64 { P0::n::f() } }
module P0::m2 { public fun f(): u64 { P0::n::f() } }

module P0::n {
    public(package) fun f(): u64 { 42 }
}

//# create-checkpoint

//# run-graphql --cursors {"i":0,"c":1} {"i":2,"c":1}

fragment ModuleFriends on Object {
    asMovePackage {
        module(name: "n") {
            # Fetch the names of all friend modules and check that there are no
            # pages on either side.
            all: friends {
                edges {
                    cursor
                    node {
                        name
                    }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # After is an exclusive lower bound, so only expect two modules in
            # the page, and an indication there's a previous page.
            after: friends(after: "@{cursor_0}") {
                edges {
                    cursor
                    node {
                        name
                    }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Before is an exclusive upper bound, so only expect two modules in
            # the page, and an indication there's a next page.
            before: friends(before: "@{cursor_1}") {
                edges {
                    cursor
                    node {
                        name
                    }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }
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
                            ...ModuleFriends
                        }
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
            # Test that we prevent overly large pages
            friends(first: 1000) {
                nodes {
                    name
                }
            }
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
                            ...ModuleFriends
                        }
                    }
                }
            }
        }
    }
}

//# run-graphql --cursors {"i":0,"c":1} {"i":2,"c":1}

fragment ModuleFriends on Object {
    asMovePackage {
        module(name: "n") {
            # Limit the number of elements in the page using `first` and skip
            # elements using `after`.
            prefix: friends(after: "@{cursor_0}", first: 1) {
                edges {
                    cursor
                    node { name }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
            prefixAll: friends(after: "@{cursor_0}", first: 2) {
                edges {
                    cursor
                    node { name }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # Limit the number of elements in the page using `last` and skip
            # elements from the end using `before`.
            suffix: friends(before: "@{cursor_1}", last: 1) {
                edges {
                    cursor
                    node { name }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }

            # This limit has no effect because there were only two nodes in the
            # page to begin with.
            suffixAll: friends(before: "@{cursor_1}", last: 2) {
                edges {
                    cursor
                    node { name }
                }
                pageInfo { hasNextPage hasPreviousPage }
            }
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
                            ...ModuleFriends
                        }
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
    public(package) fun f(): u64 { 42 }
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
            friends {
                edges {
                    cursor
                    node { name }
                }
            }
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
                            ...ModuleFriends
                        }
                    }
                }
            }
        }
    }
}
