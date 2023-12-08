// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 --accounts A --simulator

//# run-graphql

{
  chainIdentifier @deprecated
}

//# run-graphql

fragment Modules on Object  @deprecated {
    address
    asMovePackage {
        module(name: "m") {
            name
            package { asObject { address } }

            fileFormatVersion
            bytes
            disassembly
        }
    }
}

{
    transactionBlockConnection(last: 1) {
        nodes {
            effects {
                objectChanges {
                    outputState {
                        ...Modules
                    }
                }
            }
        }
    }
}
