// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// tests invalid type args

//# init --addresses test=0x0 --accounts A

//# publish
module test::m {

public struct S<phantom T: copy> {}

entry fun foo<T>() {}

}

//# run test::m::foo --type-args test::x::x

//# run test::m::foo --type-args test::m::SUI

//# run test::m::foo --type-args test::m::S

//# run test::m::foo --type-args test::m::S<u64,u8>

//# run test::m::foo --type-args test::m::S<signer>
