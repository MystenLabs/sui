// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# run-graphql

# Tests on existing system types
{
    object(address: "0x2") {
        asMovePackage {
            # Look-up a type that has generics, including phantoms.
            coin: module(name: "coin") {
                struct(name: "Coin") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    fields {
                        name
                        type {
                            repr
                            signature
                        }
                    }
                }
            }

            tx_context: module(name: "tx_context") {
                struct(name: "TxContext") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    fields {
                        name
                        type {
                            repr
                            signature
                        }
                    }
                }
            }
        }
    }
}

//# publish --upgradeable --sender A

module P0::m {
    public struct S has copy, drop { x: u64 }
}

//# create-checkpoint

//# run-graphql

# Check the contents of P0::m::S that was just published, this acts as
# a reference for when we run the same transaction against the
# upgraded package.
fragment Structs on Object {
    address
    asMovePackage {
        module(name: "m") {
            struct(name: "S") {
                name
                abilities
                typeParameters {
                    constraints
                    isPhantom
                }
                fields {
                    name
                    type {
                        repr
                        signature
                    }
                }
            }
        }
    }
}
{
    object(address: "@{obj_2_0}") {
        ...Structs
    }
}

//# upgrade --package P0 --upgrade-capability 2,1 --sender A

module P1::m {
    public struct S has copy, drop { x: u64 }
    public struct T<U: drop> { y: u64, s: S, u: U }
    public struct V { t: T<S> }
}

//# create-checkpoint

//# run-graphql

# Run a similar query as above again, but on the upgraded package, to
# see the IDs of types as they appear in the new package -- they will
# all be the runtime ID.
fragment FullStruct on MoveStruct {
    module { package { address } }
    name
    abilities
    typeParameters {
        constraints
        isPhantom
    }
    fields {
        name
        type {
            repr
            signature
        }
    }
}

fragment Structs on Object {
    address
    asMovePackage {
        module(name: "m") {
            s: struct(name: "S") { ...FullStruct }
            t: struct(name: "T") { ...FullStruct }

            # V is a special type that exists to show the
            # representations of S and T, so we don't need to query as
            # many fields for it.
            v: struct(name: "V") {
                name
                fields {
                    name
                    type { repr }
                }
            }
        }
    }
}

{
    object(address: "@{obj_5_0}") {
        ...Structs
    }
}

//# run-graphql

# But we can still confirm that we can roundtrip the `T` public struct from
# its own module, but cannot reach `T` from `S`'s defining module.

fragment ReachT on MoveStruct {
    module { struct(name: "T") { name } }
}

fragment Structs on Object {
    asMovePackage {
        module(name: "m") {
            # S should not be able to reach T from its own module
            s: struct(name: "S") { ...ReachT }

            # But T should
            t: struct(name: "T") { ...ReachT }
        }
    }
}

{
    object(address: "@{obj_5_0}") {
        ...Structs
    }
}


//# run-graphql --cursors {"n":"Coin","c":2} {"n":"TreasuryCap","c":2}
{
    object(address: "0x2") {
        asMovePackage {
            module(name: "coin") {
                # Get all the types defined in coin
                all: structs {
                    nodes {
                        name
                        fields {
                            name
                            type { repr }
                        }
                    }
                    pageInfo { hasNextPage hasPreviousPage }
                }

                # After: Coin is the first type and `after` is an
                # exclusive lower bound, so this query should indicate
                # there is a previous page, and not include `Coin` in
                # the output.
                after: structs(after: "@{cursor_0}") {
                    edges {
                        cursor
                        node { name }
                    }
                    pageInfo { hasNextPage hasPreviousPage }
                }

                # Before: Similar to `after` but at the end of the range.
                before: structs(before: "@{cursor_1}") {
                    edges {
                        cursor
                        node { name }
                    }
                    pageInfo { hasNextPage hasPreviousPage }
                }
            }
        }
    }
}

//# run-graphql --cursors {"n":"Coin","c":2} {"n":"TreasuryCap","c":2}
fragment NodeNames on MoveStructConnection {
    edges {
        cursor
        node { name }
    }
    pageInfo { hasNextPage hasPreviousPage }
}

{
    object(address: "0x2") {
        asMovePackage {
            module(name: "coin") {
                # Limit the number of elements in the page using
                # `first` and skip elements using `after`.
                prefix: structs(after: "@{cursor_0}", first: 2) {
                    ...NodeNames
                }

                # Limit has no effect because it matches the total
                # number of entries in the page.
                prefixAll: structs(after: "@{cursor_0}", first: 3) {
                    ...NodeNames
                }

                # Limit also has no effect, because it exceeds the
                # total number of entries in the page.
                prefixExcess: structs(after: "@{cursor_0}", first: 20) {
                    ...NodeNames
                }

                # Remaining tests are similar to after/first but with
                # before/last.
                suffix: structs(before: "@{cursor_1}", last: 2) {
                    ...NodeNames
                }

                suffixAll: structs(before: "@{cursor_1}", last: 3) {
                    ...NodeNames
                }

                suffixExcess: structs(before: "@{cursor_1}", last: 20) {
                    ...NodeNames
                }
            }
        }
    }
}
