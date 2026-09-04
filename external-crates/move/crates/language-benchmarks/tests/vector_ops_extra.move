// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Benches for functions affected indirectly by the bulk vector natives.

module 0x2::bench_extra {
    const SIZE_PRIM: u64 = 10_000;
    const LOOPS_PRIM: u64 = 20;
    const SIZE_BOX: u64 = 1_000;
    const LOOPS_BOX: u64 = 20;
    const BLOB: u64 = 16_384;
    const LOOPS_BLOB: u64 = 10;
    const STR: u64 = 10_000;
    const STR_IN: u64 = 1_000;
    const LOOPS_STR: u64 = 20;

    public struct Heavy has copy, drop, store {
        id: u64,
        data: vector<u64>,
        tags: vector<u8>,
    }

    fun mk_heavy(i: u64): Heavy {
        let mut data = vector[];
        let mut k = 0;
        while (k < 8) {
            data.push_back(i + k);
            k = k + 1;
        };
        let mut tags = vector[];
        k = 0;
        while (k < 16) {
            tags.push_back((k % 251) as u8);
            k = k + 1;
        };
        Heavy { id: i, data, tags }
    }

    macro fun make_vec<$T>($n: u64, $mk: |u64| -> $T): vector<$T> {
        let n = $n;
        let mut v = vector[];
        let mut i = 0;
        while (i < n) {
            v.push_back($mk(i));
            i = i + 1;
        };
        v
    }

    // ascii bytes in [0x20, 0x7e]
    fun mk_ascii(i: u64): u8 {
        (0x20 + (i % 95)) as u8
    }

    public fun bench_std_takewhile_u8() {
        // first half satisfies the predicate
        let base = make_vec!(SIZE_PRIM, |i| if (i < SIZE_PRIM / 2) 0u8 else 1u8);
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.take_while!(|e| *e == 0);
            assert!(out.length() == SIZE_PRIM / 2, 100);
            r = r + 1;
        };
    }

    public fun bench_std_skipwhile_u8() {
        let base = make_vec!(SIZE_PRIM, |i| if (i < SIZE_PRIM / 2) 0u8 else 1u8);
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.skip_while!(|e| *e == 0);
            assert!(out.length() == SIZE_PRIM - SIZE_PRIM / 2, 101);
            r = r + 1;
        };
    }

    public fun bench_std_takewhile_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let out = base.take_while!(|e| e.id < SIZE_BOX / 2);
            assert!(out.length() == SIZE_BOX / 2, 102);
            r = r + 1;
        };
    }

    public fun bench_std_skipwhile_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let out = base.skip_while!(|e| e.id < SIZE_BOX / 2);
            assert!(out.length() == SIZE_BOX - SIZE_BOX / 2, 103);
            r = r + 1;
        };
    }

    public fun bench_std_flatten_u8() {
        // 100 x 100 bytes
        let base = make_vec!(100, |_| make_vec!(100, |i| (i % 251) as u8));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.flatten();
            assert!(out.length() == 100 * 100, 104);
            r = r + 1;
        };
    }

    public fun bench_std_flatten_heavy() {
        // 50 x 20 elements
        let base = make_vec!(50, |_| make_vec!(20, |i| mk_heavy(i)));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let out = base.flatten();
            assert!(out.length() == 50 * 20, 105);
            r = r + 1;
        };
    }

    public fun bench_std_string_substring() {
        let s = std::string::utf8(make_vec!(STR, |i| mk_ascii(i)));
        let mut r = 0;
        while (r < LOOPS_STR) {
            let out = s.substring(STR / 4, STR - STR / 4);
            assert!(out.length() == STR - STR / 2, 110);
            r = r + 1;
        };
    }

    public fun bench_std_string_insert() {
        let base = std::string::utf8(make_vec!(STR, |i| mk_ascii(i)));
        let other = std::string::utf8(make_vec!(STR_IN, |i| mk_ascii(i)));
        let mut r = 0;
        while (r < LOOPS_STR) {
            let mut s = base;
            s.insert(STR / 2, other);
            assert!(s.length() == STR + STR_IN, 111);
            r = r + 1;
        };
    }

    public fun bench_std_string_append() {
        let base = std::string::utf8(make_vec!(STR, |i| mk_ascii(i)));
        let other = std::string::utf8(make_vec!(STR_IN, |i| mk_ascii(i)));
        let mut r = 0;
        while (r < LOOPS_STR) {
            let mut s = base;
            s.append(other);
            assert!(s.length() == STR + STR_IN, 112);
            r = r + 1;
        };
    }

    public fun bench_std_ascii_substring() {
        let s = std::ascii::string(make_vec!(STR, |i| mk_ascii(i)));
        let mut r = 0;
        while (r < LOOPS_STR) {
            let out = s.substring(STR / 4, STR - STR / 4);
            assert!(out.length() == STR - STR / 2, 113);
            r = r + 1;
        };
    }

    public fun bench_std_ascii_insert() {
        let base = std::ascii::string(make_vec!(STR, |i| mk_ascii(i)));
        let other = std::ascii::string(make_vec!(STR_IN, |i| mk_ascii(i)));
        let mut r = 0;
        while (r < LOOPS_STR) {
            let mut s = base;
            s.insert(STR / 2, other);
            assert!(s.length() == STR + STR_IN, 114);
            r = r + 1;
        };
    }

    // Today's sui::bcs::peel_u64 stream: reverse once, then 8 pop_backs per u64.
    public fun bench_move_bcspeel_u64() {
        let blob = make_vec!(BLOB, |i| (i % 251) as u8);
        let mut r = 0;
        while (r < LOOPS_BLOB) {
            let mut bytes = blob;
            bytes.reverse();
            let mut sum = 0u64;
            while (bytes.length() >= 8) {
                let mut value = 0u64;
                let mut i = 0u8;
                while (i < 64) {
                    let byte = bytes.pop_back() as u64;
                    value = value + (byte << i);
                    i = i + 8;
                };
                sum = sum ^ value;
            };
            assert!(sum != 1, 120);
            r = r + 1;
        };
    }

    // The s5 sui::bcs::peel_u64 stream shape: bytes forward, one drain(0, 8) per u64,
    // accumulate over indexed borrows.
    public fun bench_std_bcspeel_u64() {
        let blob = make_vec!(BLOB, |i| (i % 251) as u8);
        let mut r = 0;
        while (r < LOOPS_BLOB) {
            let mut bytes = blob;
            let mut sum = 0u64;
            while (bytes.length() >= 8) {
                let taken = bytes.drain(0, 8);
                let mut value = 0u64;
                let mut i = 0u8;
                let mut ndx = 0;
                while (i < 64) {
                    let byte = taken[ndx] as u64;
                    value = value + (byte << i);
                    i = i + 8;
                    ndx = ndx + 1;
                };
                sum = sum ^ value;
            };
            assert!(sum != 1, 122);
            r = r + 1;
        };
    }

    // The s5 sui::bcs::peel_vec_u8 shape: one drain in stream order.
    public fun bench_std_bcspeel_vecu8() {
        let blob = make_vec!(BLOB, |i| (i % 251) as u8);
        let mut r = 0;
        while (r < LOOPS_BLOB) {
            let mut bytes = blob;
            let out = bytes.drain(0, BLOB);
            assert!(out.length() == BLOB, 123);
            r = r + 1;
        };
    }

    // Today's sui::bcs::peel_vec_u8: reverse once, then one pop_back per byte.
    public fun bench_move_bcspeel_vecu8() {
        let blob = make_vec!(BLOB, |i| (i % 251) as u8);
        let mut r = 0;
        while (r < LOOPS_BLOB) {
            let mut bytes = blob;
            bytes.reverse();
            let mut out = vector<u8>[];
            let mut k = 0;
            while (k < BLOB) {
                out.push_back(bytes.pop_back());
                k = k + 1;
            };
            assert!(out.length() == BLOB, 121);
            r = r + 1;
        };
    }
}
