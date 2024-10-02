// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// test location for execution errors

//# init --addresses test=0x0

//# publish

module test::m {
    entry fun abort_() {
        // should be offset 1
        abort 0
    }

    entry fun loop_() {
        // should be offset 0
        loop {}
    }

    entry fun math() {
        // should be offset 2
        0 - 1;
    }

    entry fun vector_() {
        // should be offset 4
        std::vector::borrow(&vector[0], 1);
    }
}

//# run test::m::abort_

//# run test::m::loop_ --gas-budget 1000000

//# run test::m::math

//# run test::m::vector_
