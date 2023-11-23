// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator

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
    module { package { asObject { location } } }
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
    location
    asMovePackage {
        module(name: "m") {
            function(name: "f") { ...Signature }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...Functions
                    }
                }
            }
        }
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
    module { package { asObject { location } } }
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
    location
    asMovePackage {
        module(name: "m") {
            f: function(name: "f") { ...Signature }
            g: function(name: "g") { ...Signature }
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...Functions
                    }
                }
            }
        }
    }
}

//# run-graphql

fragment Signatures on MoveFunctionConnection {
    nodes {
        name
        typeParameters { constraints }
        parameters { repr }
        return { repr }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

{
    object(address: "0x2") {
        asMovePackage {
            module(name: "clock") {
                # Get the signatures of all functions.
                all: functionConnection { ...Signatures }

                # Functions are iterated in lexicographical order of
                # name, so this should skip the first one.
                after: functionConnection(after: "consensus_commit_prologue") {
                    ...Signatures
                }

                # ...and this should skip the last one.
                before: functionConnection(before: "timestamp_ms") {
                    ...Signatures
                }
            }
        }
    }
}

//# run-graphql

fragment Signatures on MoveFunctionConnection {
    nodes {
        name
        typeParameters { constraints }
        parameters { repr }
        return { repr }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

{
    object(address: "0x2") {
        asMovePackage {
            module(name: "clock") {
                # Limit the number of elements in the page using
                # `first` and skip elements using `after.
                prefix: functionConnection(
                    first: 1,
                    after: "consensus_commit_prologue",
                ) {
                    ...Signatures
                }

                # No limit, because there are only two other
                # functions, other than `consensus_commit_prologue`.
                prefixAll: functionConnection(
                    first: 2,
                    after: "consensus_commit_prologue",
                ) {
                    ...Signatures
                }

                # No limit, because we are asking for way more
                # functions than we have.
                prefixExcess: functionConnection(
                    first: 100,
                    after: "consensus_commit_prologue",
                ) {
                    ...Signatures
                }

                # Remaining tests are similar but replacing
                # after/first with before/last.
                suffix: functionConnection(
                    last: 1,
                    before: "timestamp_ms",
                ) {
                    ...Signatures
                }

                suffixAll: functionConnection(
                    last: 2,
                    before: "timestamp_ms",
                ) {
                    ...Signatures
                }

                suffixExcess: functionConnection(
                    last: 100,
                    before: "timestamp_ms",
                ) {
                    ...Signatures
                }
            }
        }
    }
}
