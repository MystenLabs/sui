// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --protocol-version 51 --addresses P0=0x0 P1=0x0 --accounts A --simulator 

//# publish --upgradeable --sender A
module P0::m {
    #[error]
    const ImAU8: u8 = 0;

    #[error]
    const ImAU16: u16 = 1;

    #[error]
    const ImAU32: u32 = 2;

    #[error]
    const ImAU64: u64 = 3;

    #[error]
    const ImAU128: u128 = 4;

    #[error]
    const ImAU256: u256 = 5;

    #[error]
    const ImABool: bool = true;

    #[error]
    const ImAnAddress: address = @6;

    #[error]
    const ImAString: vector<u8> = b"This is a string";

    #[error]
    const ImNotAString: vector<u64> = vector[1,2,3,4,5];

    public fun callU8() {
        abort ImAU8
    }

    public fun callU16() {
        abort ImAU16
    }

    public fun callU32() {
        abort ImAU32
    }

    public fun callU64() {
        abort ImAU64
    }

    public fun callU128() {
        abort ImAU128
    }

    public fun callU256() {
        abort ImAU256
    }

    public fun callAddress() {
        abort ImAnAddress
    }

    public fun callString() {
        abort ImAString
    }

    public fun callU64vec() {
        abort ImNotAString
    }

    public fun normalAbort() {
        abort 0
    }

    public fun assertLineNo() {
        assert!(false);
    }
}

//# run P0::m::callU8

//# run P0::m::callU16

//# run P0::m::callU32

//# run P0::m::callU64

//# run P0::m::callU128

//# run P0::m::callU256

//# run P0::m::callAddress

//# run P0::m::callString

//# run P0::m::callU64vec

//# run P0::m::normalAbort

//# run P0::m::assertLineNo

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(last: 11) {
    nodes {
      effects {
        status
        errors
      }
    }
  }
}

//# upgrade --package P0 --upgrade-capability 1,1 --sender A
// Upgrade the module with new error values but using the same constant names
// (etc) to make sure we properly resolve the module location for clever
// errors.
module P0::m {
    #[error]
    const ImAU8: u8 = 7;

    #[error]
    const ImAU16: u16 = 8;

    #[error]
    const ImAU32: u32 = 9;

    #[error]
    const ImAU64: u64 = 10;

    #[error]
    const ImAU128: u128 = 11;

    #[error]
    const ImAU256: u256 = 12;

    #[error]
    const ImABool: bool = false;

    #[error]
    const ImAnAddress: address = @13;

    #[error]
    const ImAString: vector<u8> = b"This is a string in v2";

    #[error]
    const ImNotAString: vector<u64> = vector[1,2,3,4,5,6];

    public fun callU8() {
        abort ImAU8
    }

    public fun callU16() {
        abort ImAU16
    }

    public fun callU32() {
        abort ImAU32
    }

    public fun callU64() {
        abort ImAU64
    }

    public fun callU128() {
        abort ImAU128
    }

    public fun callU256() {
        abort ImAU256
    }

    public fun callAddress() {
        abort ImAnAddress
    }

    public fun callString() {
        abort ImAString
    }

    public fun callU64vec() {
        abort ImNotAString
    }

    public fun normalAbort() {
        abort 0
    }

    public fun assertLineNo() {
        assert!(false);
    }
}

//# run P0::m::callU8

//# run P0::m::callU16

//# run P0::m::callU32

//# run P0::m::callU64

//# run P0::m::callU128

//# run P0::m::callU256

//# run P0::m::callAddress

//# run P0::m::callString

//# run P0::m::callU64vec

//# run P0::m::normalAbort

//# run P0::m::assertLineNo

//# create-checkpoint

//# run-graphql
{
  transactionBlocks(last: 9) {
    nodes {
      effects {
        status
        errors
      }
    }
  }
}
