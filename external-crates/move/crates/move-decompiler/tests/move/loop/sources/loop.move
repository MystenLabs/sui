// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module loop_addr::loop_mod;

public fun do_0(_cond: &mut bool) {  }
public fun do_1(_cond: &mut bool) {  }
public fun do_2(_cond: &mut bool) {  }
public fun do_3(_cond: &mut bool) {  }
public fun do_4(_cond: &mut bool) {  }

public fun loop_0(cond: &mut bool) {
    while (*cond) {
        do_0(cond);
    };
    do_1(cond);
}

public fun loop_1(cond: &mut bool) {
    do_0(cond);
    while (*cond) {
        do_1(cond);
    };
    do_2(cond);
}


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
