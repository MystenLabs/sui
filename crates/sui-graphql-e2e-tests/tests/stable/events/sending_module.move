// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 62 --addresses P=0x0 --accounts A --simulator

//# publish --upgradeable --sender A
module P::M0 {
  public struct Event has copy, drop {
    value: u64
  }
}

//# upgrade --package P --upgrade-capability 1,1 --sender A
module P::M0 {
  public struct Event has copy, drop {
    value: u64
  }

  public fun emit() {
    sui::event::emit(Event { value: 42 })
  }
}

module P::M1 {
  public fun emit() {
    P::M0::emit()
  }
}

//# run P::M1::emit --sender A

//# create-checkpoint

//# run-graphql
{
  events {
    nodes {
      sendingModule {
        package { address }
        name
      }
    }
  }
}
