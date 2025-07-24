// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module loops::loops {
    
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

    public fun while_test() {
        let mut i = 0;
        while (i < 10) {
            i = i + 1;
        };
    }    

}