// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# run-graphql

# Tests on existing system types
fragment Signature on MoveFunction {
    name
    visibility
    isEntry
    typeParameters {
        constraints
    }
    parameters { repr }
    return { repr }
}

{
    object(address: "0x2") {
        asMovePackage {
            coin: module(name: "coin") {
                # A public function
                total_supply: function(name: "total_supply") { ...Signature }

                # An entry function
                join: function(name: "join") { ...Signature }

            }

            sui: module(name: "sui") {
                # A private function
                new: function(name: "new") { ...Signature }
            }
        }
    }
}

//# publish --upgradeable --sender A

module P0::m {
    public fun f<T: drop>(_: T): (u64, u64) { (42, 43) }
}

//# create-checkpoint

//# run-graphql

# Get the signature of a function published in a third-party package
fragment Signature on MoveFunction {
    module { package { address } }
    name
    visibility
    isEntry
    typeParameters {
        constraints
    }
    parameters { repr }
    return { repr }
}

fragment Functions on Object {
    address
    asMovePackage {
        module(name: "m") {
            function(name: "f") { ...Signature }
        }
    }
}

{
    object(address: "@{obj_2_0}") {
        ...Functions
    }
}


//# upgrade --package P0 --upgrade-capability 2,1 --sender A

module P0::m {
    public fun f<T: drop>(_: T): (u64, u64) { (42, 43) }
    entry fun g(): u64 { let (x, y) = f<u32>(44); x + y }
}

//# create-checkpoint

//# run-graphql

# Get the signature of a function published in a third-party package
fragment Signature on MoveFunction {
    module { package { address } }
    name
    visibility
    isEntry
    typeParameters {
        constraints
    }
    parameters { repr }
    return { repr }
}

fragment Functions on Object {
    address
    asMovePackage {
        module(name: "m") {
            f: function(name: "f") { ...Signature }
            g: function(name: "g") { ...Signature }
        }
    }
}

{
    object(address: "@{obj_5_0}") {
        ...Functions
    }
}

//# run-graphql --cursors {"n":"consensus_commit_prologue","c":2} {"n":"timestamp_ms","c":2}

fragment Signatures on MoveFunctionConnection {
    edges {
        cursor
        node {
            name
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

{
    object(address: "0x2") {
        asMovePackage {
            module(name: "clock") {
                # Get the signatures of all functions.
                all: functions { ...Signatures }

                # Functions are iterated in lexicographical order of
                # name, so this should skip the first one.
                after: functions(after: "@{cursor_0}") {
                    ...Signatures
                }

                # ...and this should skip the last one.
                before: functions(before: "@{cursor_1}") {
                    ...Signatures
                }
            }
        }
    }
}

//# run-graphql --cursors {"n":"consensus_commit_prologue","c":2} {"n":"timestamp_ms","c":2}

fragment Signatures on MoveFunctionConnection {
    edges {
        cursor
        node {
            name
            typeParameters { constraints }
            parameters { repr }
            return { repr }
        }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

{
    object(address: "0x2") {
        asMovePackage {
            module(name: "clock") {
                # Limit the number of elements in the page using
                # `first` and skip elements using `after.
                prefix: functions(
                    first: 1,
                    after: "@{cursor_0}",
                ) {
                    ...Signatures
                }

                # No limit, because there are only two other
                # functions, other than `consensus_commit_prologue`.
                prefixAll: functions(
                    first: 2,
                    after: "@{cursor_0}",
                ) {
                    ...Signatures
                }

                # No limit, because we are asking for way more
                # functions than we have.
                prefixExcess: functions(
                    first: 20,
                    after: "@{cursor_0}",
                ) {
                    ...Signatures
                }

                # Remaining tests are similar but replacing
                # after/first with before/last.
                suffix: functions(
                    last: 1,
                    before: "@{cursor_1}",
                ) {
                    ...Signatures
                }

                suffixAll: functions(
                    last: 2,
                    before: "@{cursor_1}",
                ) {
                    ...Signatures
                }

                suffixExcess: functions(
                    last: 20,
                    before: "@{cursor_1}",
                ) {
                    ...Signatures
                }
            }
        }
    }
}
