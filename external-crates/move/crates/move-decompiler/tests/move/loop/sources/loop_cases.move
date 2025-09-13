// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module addr::loop_cases {
    
    public fun loop_test() {
        let mut i = 0;
        loop {
            i = i + 1;
            if (i >= 10) break
        }
    }

    public fun loop_test_2() {
        let mut x = 0;
        loop {
            x = x + 1;
            if (x % 2 == 1) continue;
            if (x == 10) break
        }
    }

    public fun loop_test_3() {
        let mut i = 0;
        loop {
            if (i >= 10) break;
            i = i + 1;
        }
    }

    public fun loop_test_4() {
        let i = 10;
        loop {
            if (i >= 10) {
                continue
            };
            break;
        }
    }

    public fun loop_test_5() {
        loop {
            continue
        }
    }

    public fun loop_test_6(mut cond: u64) {
        let mut x = 0;
        loop {
            if (cond > 10) {
                x = x + 1;
                cond = cond - 1;
            } else {
                break
            }
        }
    }

    public fun loop_test_7(foo: u64): u64 {
        let mut bar = 10;
        loop {
            bar = bar * foo;
            break
        };
        bar
    }

    public fun loop_test_8() {
        let mut i = 0;
        let mut j = 5; 
        loop {
            i = i + 1;
            j = j * 2 + i;
            if (i >= 10) break
        }
    }

    public fun loop_test_9() {
        let mut i = 0;
        let mut j = 5; 
        loop {
            i = i + 1;
            j = j * 2 + i;
            if (j - i %3 == 0) {
                j = 100
            };
            if (i >= 10) break
        }
    }

}