// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 --accounts A --simulator

//# publish --sender A
module P0::m {
    macro fun const_assert() {
        assert!(false, EMsg) // putting this early to make the line number clear
    }

    #[error]
    const EMsg: vector<u8> = b"This is a string";

    macro fun a() {
        assert!(false)
    }

    macro fun calls_a() {
        a!()
    }

    entry fun t_a() {
        a!() // assert should point to this line
    }

    entry fun t_calls_a() {
        calls_a!() // assert should point to this line
    }

    entry fun t_const_assert() {
        const_assert!() // this assert will _not_ have its line number changed
    }
}

//# run P0::m::t_a

//# run P0::m::t_calls_a

//# run P0::m::t_const_assert

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(last: 3) {
    nodes {
      effects {
        status
        errors
      }
    }
  }
}
