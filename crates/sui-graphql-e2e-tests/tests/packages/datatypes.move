// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator

//# run-graphql

# Tests on existing system types
{
    object(address: "0x2") {
        asMovePackage {
            # Look-up a type that has generics, including phantoms.
            coin: module(name: "coin") {
                datatype(name: "Coin") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    asMoveStruct {
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

            tx_context: module(name: "tx_context") {
                datatype(name: "TxContext") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    asMoveStruct {
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
}


