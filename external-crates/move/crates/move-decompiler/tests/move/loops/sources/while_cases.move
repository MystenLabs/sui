// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

module loops::while_cases;

public fun while_test() {
    let mut i = 0;
    while (i < 10) {
        i = i + 1;
    };
}

public fun while_test_2() {
    let mut i = 0;
    while (i < 10 || i == 7) {
        i = i + 1;
    };
}

public fun while_test_3() {
    let mut i = 0;
    while (i < 10) {
        if (i % 2 == 0) {
            i = i + 1;
        } else {
            i = i + 2;
        }
    };
}

public fun while_test_4() {
    let mut i = 0;
    while (i < 10 || i == 7) {
        if (i % 2 == 0) {
            i = i + 1;
        } else {
            i = i + 2;
        }
    };
}

public fun while_test_5() {
    let mut i = 0;
    let mut j = 0;
    let mut v = 0;
    while (i < 10) {
        while( j < 10) {
            v = v + (i * j + j);
            j = j + 1;
        };
        i = i + 1;
    }
}

public fun while_test_6() {
    let mut i = 0;
    while (i < 10 || i == 7) {
        i = i + 1;
    };
}

