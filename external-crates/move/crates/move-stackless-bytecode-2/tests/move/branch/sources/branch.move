// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module branch::branch {

    public fun is_even(x: u64): u64 {
        let z = 10;
        let k = 13;
        let y;
        if (x % 2 == 0 ) {
            y = z + 20;
        } else {
            y = z + 30;
        };
        return y * k
    }

    public fun conditions(x: u64, y: u64): u64 {
        if (x > 10 && y < 20 || x < 30) {
            x + y
        } else {
            x * y
        }

    }

}
