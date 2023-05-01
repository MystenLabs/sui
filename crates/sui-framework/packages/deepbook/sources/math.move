// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module deepbook::math {
    /// scaling setting for float
    const FLOAT_SCALING: u64 = 1_000_000_000;
    const FLOAT_SCALING_U128: u128 = 1_000_000_000;

    public fun mul(x: u64, y: u64): u64 {
        let x = (x as u128);
        let y = (y as u128);
        ((x * y / FLOAT_SCALING_U128) as u64)
    }

    public fun mul_round(x: u64, y: u64): (bool, u64) {
        let x = (x as u128);
        let y = (y as u128);
        let is_round_down = true;
        if ((x * y) % FLOAT_SCALING_U128 == 0) is_round_down = false;
        (is_round_down, ((x * y / FLOAT_SCALING_U128) as u64))
    }

    public fun div_round(x: u64, y: u64): (bool, u64) {
        let x = (x as u128);
        let y = (y as u128);
        let is_round_down = true;
        if ((x * (FLOAT_SCALING as u128) % y) == 0) is_round_down = false;
        (is_round_down, ((x * (FLOAT_SCALING as u128) / y) as u64))
    }
}
