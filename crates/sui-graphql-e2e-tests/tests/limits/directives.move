// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 48 --addresses Test=0x0 --accounts A --simulator

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
            package { address }

            fileFormatVersion
            bytes
            disassembly
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
                        ...Modules
                    }
                  }
                }
            }
        }
    }
}

//# run-graphql

{
  chainIdentifier @skip(if: true)
}

//# run-graphql

{
  chainIdentifier @skip(if: false)
}

//# run-graphql

{
  chainIdentifier @include(if: true)
}

//# run-graphql

{
  chainIdentifier @include(if: false)
}
