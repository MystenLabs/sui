// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator --protocol-version 51

//# publish --upgradeable --sender A

module P0::m {
    public struct Bar {
        x: u64,
        y: bool,
    }
    public enum S has copy, drop {
        V1(u64),
        V2 { x: bool, y: u64 },
    }
}

//# create-checkpoint

//# run-graphql

# Check the contents of P0::m::S that was just published, this acts as
# a reference for when we run the same transaction against the
# upgraded package.
fragment Enums on Object {
    address
    asMovePackage {
        module(name: "m") {
            enum(name: "S") {
                name
                abilities
                typeParameters {
                    constraints
                    isPhantom
                }
                variants {
                    name
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

{
    object(address: "@{obj_1_0}") {
        ...Enums
    }
}

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
module P0::m {
    public struct Bar {
        x: u64,
        y: bool,
    }
    public enum S has copy, drop {
        V1(u64),
        V2 { x: bool, y: u64 },
    }

    public enum T<U: drop> { VV { y: u64, s: S, u: U } }
    public enum V { V { t: T<S> } }
}

//# create-checkpoint

//# run-graphql

# Run a similar query as above again, but on the upgraded package, to
# see the IDs of types as they appear in the new package -- they will
# all be the runtime ID.
fragment FullEnum on MoveEnum {
    module { package { address } }
    name
    abilities
    typeParameters {
        constraints
        isPhantom
    }
    variants {
        name
        fields {
            name
            type {
                repr
                signature
            }
        }
    }
}

fragment Enums on Object {
    address
    asMovePackage {
        module(name: "m") {
            s: enum(name: "S") { ...FullEnum }
            t: enum(name: "T") { ...FullEnum }

            # V is a special type that exists to show the
            # representations of S and T, so we don't need to query as
            # many fields for it.
            v: enum(name: "V") {
                name
                variants {
                    name
                    fields {
                        name
                        type { repr }
                    }
                }
            }
        }
    }
}

{
    object(address: "@{obj_4_0}") {
        ...Enums
    }
}

//# run-graphql

# But we can still confirm that we can roundtrip the `T` public enum from
# its own module, but cannot reach `T` from `S`'s defining module.

fragment ReachT on MoveEnum {
    module { enum(name: "T") { name } }
}

fragment Enums on Object {
    asMovePackage {
        module(name: "m") {
            # S should not be able to reach T from its own module
            s: enum(name: "S") { ...ReachT }

            # But T should
            t: enum(name: "T") { ...ReachT }
        }
    }
}

{
    object(address: "@{obj_4_0}") {
        ...Enums
    }
}
