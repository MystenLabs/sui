// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses P0=0x0 P1=0x0 --accounts A --simulator --protocol-version 51

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

//# publish --upgradeable --sender A
module P0::m {
    public enum IsAnEnum has copy, drop {
        V1(u64),
        V2 { x: bool, y: u64 },
    }
    public struct IsAStruct {
        x: u64,
        y: bool,
    }
}

//# create-checkpoint

//# view-object 2,0

//# run-graphql
# Get all datatypes in the module and print out their common fields
fragment Datatypes on Object {
   address
   asMovePackage {
       module(name: "m") {
           datatypes {
               nodes {
                   name
                   abilities
                   typeParameters {
                       constraints
                       isPhantom
                   }
               }
               pageInfo { hasNextPage hasPreviousPage }
           }
       }
   }
}
{
    object(address: "@{obj_2_0}") {
        ...Datatypes
    }
}

//# run-graphql
# Get all datatypes in the module, print out their common fields, and then try to cast them either to an enum or a struct.
fragment Datatypes on Object {
   address
   asMovePackage {
       module(name: "m") {
           datatypes {
               nodes {
                   name
                   abilities
                   typeParameters {
                       constraints
                       isPhantom
                   }
                   asMoveEnum {
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
               pageInfo { hasNextPage hasPreviousPage }
           }
       }
   }
}
{
    object(address: "@{obj_2_0}") {
        ...Datatypes
    }
}

//# run-graphql
# Get a specific datatype (that's an enum) and cast it to an enum.
{
    object(address: "@{obj_2_0}") {
        asMovePackage {
            module(name: "m") {
                datatype(name: "IsAnEnum") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    asMoveEnum {
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
    }
}

//# run-graphql
# Get a specific datatype (that's a struct) and cast it to an enum (should be null).
{
    object(address: "@{obj_2_0}") {
        asMovePackage {
            module(name: "m") {
                datatype(name: "IsAStruct") {
                    name
                    abilities
                    typeParameters {
                        constraints
                        isPhantom
                    }
                    # Should be null
                    asMoveEnum {
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
    }
}
