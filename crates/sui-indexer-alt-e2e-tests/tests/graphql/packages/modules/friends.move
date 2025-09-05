// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 70 --accounts A --addresses P=0x0 --simulator

//# publish
module P::M {
  public(package) fun f(): u64 { 42 }
}

module P::N {
  public(package) fun g(): u64 { P::M::f() }
}

module P::O {
  public fun h(): u64 { P::M::f() }
}

module P::P {
  public fun i(): u64 { P::N::g() }
}

//# create-checkpoint

//# run-graphql
{
  package(address: "@{P}") {
    modules {
      nodes {
        name
        friends {
          nodes { name }
        }
      }
    }
  }
}
