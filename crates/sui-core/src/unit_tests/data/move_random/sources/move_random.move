// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module examples::move_random {
    // simple infinite loop to go out of gas in computation
    public entry fun loopy() {
        loop { }
    }
}
