// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

//# init --addresses Test=0x0 A=0x42

//# publish
module Test::M1;
#[error(code = 10)]
const EError: vector<u8> = b"An error occurred";

public fun foo() { abort EError }

//# run Test::M1::foo
