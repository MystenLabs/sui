// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public fun foo(): u64 {
    42
  }
}

//# create-checkpoint

//# run-graphql
{ # Fetch a package as an object and convert it to a MovePackage. Note that
  # `objectBcs` and `packageBcs` are different.
  object(address: "@{P}", version: 1) {
    objectBcs
    asMovePackage {
      objectBcs
      packageBcs
    }
  }
}

//# run-graphql
{ # Fetch an object and try to convert it to a MovePackage, which will fail.
  object(address: "@{obj_0_0}", version: 1) {
    objectBcs
    asMovePackage {
      objectBcs
      packageBcs
    }
  }

}
