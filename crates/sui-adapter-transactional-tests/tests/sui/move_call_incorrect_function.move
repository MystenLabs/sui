// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// These functions do not exist

//# init --addresses Test=0x0

//# publish

module Test::M {
    public entry fun create(_value: u64, _recipient: address) {}

}

// Instead of calling on the Test package, we are calling a non-existent package
//# run 0x242::M::create

// Calling a non-existent function.
//# run Test::M::foo
