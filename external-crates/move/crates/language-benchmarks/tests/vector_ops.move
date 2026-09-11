// Copyright (c) Mysten Labs, Inc.
// SPDX-License-Identifier: Apache-2.0

// Bulk vector op benchmarks: bench_move_* are pure-Move implementations, bench_std_* the
// std::vector entry points, bench_setup_* the per-repetition scaffolding for subtraction.
module 0x2::bench {
    const SIZE_PRIM: u64 = 10_000;
    const LOOPS_PRIM: u64 = 20;
    const SIZE_BOX: u64 = 1_000;
    const LOOPS_BOX: u64 = 20;

    const SIZE_SMALL: u64 = 256;
    const LOOPS_SMALL: u64 = 400;
    const SIZE_LARGE: u64 = 200_000;
    const LOOPS_LARGE: u64 = 4;
    const SIZE_BOX_SMALL: u64 = 64;
    const LOOPS_BOX_SMALL: u64 = 400;
    const SIZE_BOX_LARGE: u64 = 8_000;
    const LOOPS_BOX_LARGE: u64 = 4;

    public struct Light has copy, drop, store {
        a: u64,
    }

    public struct Inner has copy, drop, store {
        x: u128,
        y: vector<address>,
    }

    public struct Heavy has copy, drop, store {
        id: u64,
        data: vector<u64>,
        tags: vector<u8>,
        inner: Inner,
    }


    fun mk_u8(i: u64): u8 {
        (i % 251) as u8
    }

    fun mk_u64(i: u64): u64 {
        i
    }

    fun mk_u128(i: u64): u128 {
        i as u128
    }

    fun mk_u256(i: u64): u256 {
        i as u256
    }

    fun mk_address(_i: u64): address {
        @0xA11CE
    }

    fun mk_light(i: u64): Light {
        Light { a: i }
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
        Heavy {
            id: i,
            data,
            tags,
            inner: Inner { x: i as u128, y: vector[@0x1, @0x2, @0x3, @0x4] },
        }
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


    fun append_move<T>(v: &mut vector<T>, mut other: vector<T>) {
        other.reverse();
        while (!other.is_empty()) v.push_back(other.pop_back());
        other.destroy_empty();
    }

    fun drain_move<T>(v: &mut vector<T>, i: u64, j: u64): vector<T> {
        let len = v.length();
        assert!(i <= j && j <= len, 0);
        let mut tail = vector[];
        let mut k = len;
        while (k > j) {
            tail.push_back(v.pop_back());
            k = k - 1;
        };
        let mut removed = vector[];
        k = j;
        while (k > i) {
            removed.push_back(v.pop_back());
            k = k - 1;
        };
        removed.reverse();
        while (!tail.is_empty()) v.push_back(tail.pop_back());
        tail.destroy_empty();
        removed
    }

    fun truncate_move<T: drop>(v: &mut vector<T>, n: u64) {
        assert!(n <= v.length(), 0);
        while (v.length() > n) {
            v.pop_back();
        };
    }

    fun slice_move<T: copy>(v: &vector<T>, i: u64, j: u64): vector<T> {
        assert!(i <= j && j <= v.length(), 0);
        let mut r = vector[];
        let mut k = i;
        while (k < j) {
            r.push_back(v[k]);
            k = k + 1;
        };
        r
    }

    fun splice_move<T>(v: &mut vector<T>, i: u64, j: u64, mut other: vector<T>): vector<T> {
        let len = v.length();
        assert!(i <= j && j <= len, 0);
        let mut tail = vector[];
        let mut k = len;
        while (k > j) {
            tail.push_back(v.pop_back());
            k = k - 1;
        };
        let mut removed = vector[];
        k = j;
        while (k > i) {
            removed.push_back(v.pop_back());
            k = k - 1;
        };
        removed.reverse();
        other.reverse();
        while (!other.is_empty()) v.push_back(other.pop_back());
        other.destroy_empty();
        while (!tail.is_empty()) v.push_back(tail.pop_back());
        tail.destroy_empty();
        removed
    }


    macro fun run_append<$T>(
        $size: u64,
        $loops: u64,
        $mk: |u64| -> $T,
        $append: |&mut vector<$T>, vector<$T>|,
    ) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let other = make_vec!(size, |i| $mk(i));
        let mut r = 0;
        while (r < loops) {
            let mut v = base;
            $append(&mut v, other);
            assert!(v.length() == 2 * size, 100);
            r = r + 1;
        };
    }

    macro fun run_drain<$T>(
        $size: u64,
        $loops: u64,
        $mk: |u64| -> $T,
        $drain: |&mut vector<$T>, u64, u64| -> vector<$T>,
    ) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let i = size / 4;
        let j = size - size / 4;
        let mut r = 0;
        while (r < loops) {
            let mut v = base;
            let out = $drain(&mut v, i, j);
            assert!(v.length() == size - (j - i), 101);
            assert!(out.length() == j - i, 102);
            r = r + 1;
        };
    }

    macro fun run_truncate<$T: drop>(
        $size: u64,
        $loops: u64,
        $mk: |u64| -> $T,
        $truncate: |&mut vector<$T>, u64|,
    ) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let n = size / 2;
        let mut r = 0;
        while (r < loops) {
            let mut v = base;
            $truncate(&mut v, n);
            assert!(v.length() == n, 103);
            r = r + 1;
        };
    }

    macro fun run_slice<$T: copy>(
        $size: u64,
        $loops: u64,
        $mk: |u64| -> $T,
        $slice: |&vector<$T>, u64, u64| -> vector<$T>,
    ) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let i = size / 4;
        let j = size - size / 4;
        let mut r = 0;
        while (r < loops) {
            let out = $slice(&base, i, j);
            assert!(out.length() == j - i, 104);
            r = r + 1;
        };
    }

    // General splice shape: removes [i, j), inserts `osize` elements.
    macro fun run_splice<$T>(
        $size: u64,
        $loops: u64,
        $i: u64,
        $j: u64,
        $osize: u64,
        $mk: |u64| -> $T,
        $splice: |&mut vector<$T>, u64, u64, vector<$T>| -> vector<$T>,
    ) {
        let size = $size;
        let loops = $loops;
        let i = $i;
        let j = $j;
        let osize = $osize;
        let base = make_vec!(size, |i| $mk(i));
        let other = make_vec!(osize, |i| $mk(i));
        let mut r = 0;
        while (r < loops) {
            let mut v = base;
            let out = $splice(&mut v, i, j, other);
            assert!(v.length() == size - (j - i) + osize, 105);
            assert!(out.length() == j - i, 106);
            r = r + 1;
        };
    }

    // === setup-only twins (for subtraction) ===

    // one input copy per repetition
    macro fun run_copy1<$T>($size: u64, $loops: u64, $mk: |u64| -> $T) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let mut r = 0;
        while (r < loops) {
            let v = base;
            assert!(v.length() == size, 107);
            r = r + 1;
        };
    }

    // two input copies per repetition
    macro fun run_copy2<$T>($size: u64, $loops: u64, $mk: |u64| -> $T) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let other = make_vec!(size, |i| $mk(i));
        let mut r = 0;
        while (r < loops) {
            let v = base;
            let o = other;
            assert!(v.length() == size && o.length() == size, 108);
            r = r + 1;
        };
    }

    // no per-repetition copies: input build + loop overhead only
    macro fun run_noop<$T>($size: u64, $loops: u64, $mk: |u64| -> $T) {
        let size = $size;
        let loops = $loops;
        let base = make_vec!(size, |i| $mk(i));
        let mut r = 0;
        while (r < loops) {
            assert!(base.length() == size, 109);
            r = r + 1;
        };
    }

    // === append ===

    public fun bench_move_append_u8() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_u64() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_u128() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_u256() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_address() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_light() {
        run_append!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_heavy() {
        run_append!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, o| append_move(v, o))
    }

    public fun bench_std_append_u8() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, o| v.append(o))
    }

    public fun bench_std_append_u64() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, o| v.append(o))
    }

    public fun bench_std_append_u128() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, o| v.append(o))
    }

    public fun bench_std_append_u256() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, o| v.append(o))
    }

    public fun bench_std_append_address() {
        run_append!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, o| v.append(o))
    }

    public fun bench_std_append_light() {
        run_append!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, o| v.append(o))
    }

    public fun bench_std_append_heavy() {
        run_append!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, o| v.append(o))
    }

    // === drain ===

    public fun bench_move_drain_u8() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_u64() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_u128() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_u256() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_address() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_light() {
        run_drain!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, i, j| drain_move(v, i, j))
    }

    public fun bench_move_drain_heavy() {
        run_drain!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, i, j| drain_move(v, i, j))
    }

    // === truncate ===

    public fun bench_move_truncate_u8() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_u64() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_u128() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_u256() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_address() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_light() {
        run_truncate!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, n| truncate_move(v, n))
    }

    public fun bench_move_truncate_heavy() {
        run_truncate!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, n| truncate_move(v, n))
    }

    // === slice ===

    public fun bench_move_slice_u8() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_u64() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_u128() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_u256() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_address() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_light() {
        run_slice!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, i, j| slice_move(v, i, j))
    }

    public fun bench_move_slice_heavy() {
        run_slice!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, i, j| slice_move(v, i, j))
    }

    // === drain (std) ===

    public fun bench_std_drain_u8() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_u64() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_u128() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_u256() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_address() {
        run_drain!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_light() {
        run_drain!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, i, j| v.drain(i, j))
    }

    public fun bench_std_drain_heavy() {
        run_drain!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, i, j| v.drain(i, j))
    }

    // === truncate (std) ===

    public fun bench_std_truncate_u8() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_u64() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_u128() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_u256() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_address() {
        run_truncate!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_light() {
        run_truncate!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, n| v.truncate(n))
    }

    public fun bench_std_truncate_heavy() {
        run_truncate!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, n| v.truncate(n))
    }

    // === slice (std) ===

    public fun bench_std_slice_u8() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_u64() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_u128() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_u256() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_address() {
        run_slice!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_light() {
        run_slice!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i), |v, i, j| v.slice(i, j))
    }

    public fun bench_std_slice_heavy() {
        run_slice!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i), |v, i, j| v.slice(i, j))
    }

    // === splice (default shape: remove middle quarter, insert half-size => net growth) ===

    public fun bench_move_splice_u8() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u64() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u64(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u128() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u128(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u256() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u256(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_address() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_address(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_light() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_light(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    // === splice (std) ===

    public fun bench_std_splice_u8() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u64() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u64(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u128() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u128(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u256() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u256(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_address() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_address(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_light() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_light(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    // === splice shapes, std (u8 and heavy) ===

    public fun bench_std_splice_u8_insert_mid() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u8_drain_tail() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 2,
            SIZE_PRIM,
            0,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u8_exact_replace() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 4 + SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u8_append_shape() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM,
            SIZE_PRIM,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u8_whole_swap() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            0,
            SIZE_PRIM,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_insert_mid() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_drain_tail() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 2,
            SIZE_BOX,
            0,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_exact_replace() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 4 + SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_append_shape() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX,
            SIZE_BOX,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_whole_swap() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            0,
            SIZE_BOX,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    // === splice size sweep, std (u8 and heavy) ===

    public fun bench_std_splice_u8_small() {
        run_splice!(
            SIZE_SMALL,
            LOOPS_SMALL,
            SIZE_SMALL / 4,
            SIZE_SMALL / 2,
            SIZE_SMALL / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_u8_large() {
        run_splice!(
            SIZE_LARGE,
            LOOPS_LARGE,
            SIZE_LARGE / 4,
            SIZE_LARGE / 2,
            SIZE_LARGE / 2,
            |i| mk_u8(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_small() {
        run_splice!(
            SIZE_BOX_SMALL,
            LOOPS_BOX_SMALL,
            SIZE_BOX_SMALL / 4,
            SIZE_BOX_SMALL / 2,
            SIZE_BOX_SMALL / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_splice_heavy_large() {
        run_splice!(
            SIZE_BOX_LARGE,
            LOOPS_BOX_LARGE,
            SIZE_BOX_LARGE / 4,
            SIZE_BOX_LARGE / 2,
            SIZE_BOX_LARGE / 2,
            |i| mk_heavy(i),
            |v, i, j, o| v.splice(i, j, o),
        )
    }

    public fun bench_std_append_heavy_small() {
        run_append!(SIZE_BOX_SMALL, LOOPS_BOX_SMALL, |i| mk_heavy(i), |v, o| v.append(o))
    }

    public fun bench_std_append_heavy_large() {
        run_append!(SIZE_BOX_LARGE, LOOPS_BOX_LARGE, |i| mk_heavy(i), |v, o| v.append(o))
    }

    // === splice shapes (u8 and heavy) ===

    public fun bench_move_splice_u8_insert_mid() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u8_drain_tail() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 2,
            SIZE_PRIM,
            0,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u8_exact_replace() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM / 4,
            SIZE_PRIM / 4 + SIZE_PRIM / 2,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u8_append_shape() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            SIZE_PRIM,
            SIZE_PRIM,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u8_whole_swap() {
        run_splice!(
            SIZE_PRIM,
            LOOPS_PRIM,
            0,
            SIZE_PRIM,
            SIZE_PRIM / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_insert_mid() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_drain_tail() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 2,
            SIZE_BOX,
            0,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_exact_replace() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX / 4,
            SIZE_BOX / 4 + SIZE_BOX / 2,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_append_shape() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            SIZE_BOX,
            SIZE_BOX,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_whole_swap() {
        run_splice!(
            SIZE_BOX,
            LOOPS_BOX,
            0,
            SIZE_BOX,
            SIZE_BOX / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    // === size sweep (append and splice; u8 and heavy) ===

    public fun bench_move_append_u8_small() {
        run_append!(SIZE_SMALL, LOOPS_SMALL, |i| mk_u8(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_u8_large() {
        run_append!(SIZE_LARGE, LOOPS_LARGE, |i| mk_u8(i), |v, o| append_move(v, o))
    }

    public fun bench_std_append_u8_small() {
        run_append!(SIZE_SMALL, LOOPS_SMALL, |i| mk_u8(i), |v, o| v.append(o))
    }

    public fun bench_std_append_u8_large() {
        run_append!(SIZE_LARGE, LOOPS_LARGE, |i| mk_u8(i), |v, o| v.append(o))
    }

    public fun bench_move_append_heavy_small() {
        run_append!(SIZE_BOX_SMALL, LOOPS_BOX_SMALL, |i| mk_heavy(i), |v, o| append_move(v, o))
    }

    public fun bench_move_append_heavy_large() {
        run_append!(SIZE_BOX_LARGE, LOOPS_BOX_LARGE, |i| mk_heavy(i), |v, o| append_move(v, o))
    }

    public fun bench_move_splice_u8_small() {
        run_splice!(
            SIZE_SMALL,
            LOOPS_SMALL,
            SIZE_SMALL / 4,
            SIZE_SMALL / 2,
            SIZE_SMALL / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_u8_large() {
        run_splice!(
            SIZE_LARGE,
            LOOPS_LARGE,
            SIZE_LARGE / 4,
            SIZE_LARGE / 2,
            SIZE_LARGE / 2,
            |i| mk_u8(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_small() {
        run_splice!(
            SIZE_BOX_SMALL,
            LOOPS_BOX_SMALL,
            SIZE_BOX_SMALL / 4,
            SIZE_BOX_SMALL / 2,
            SIZE_BOX_SMALL / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    public fun bench_move_splice_heavy_large() {
        run_splice!(
            SIZE_BOX_LARGE,
            LOOPS_BOX_LARGE,
            SIZE_BOX_LARGE / 4,
            SIZE_BOX_LARGE / 2,
            SIZE_BOX_LARGE / 2,
            |i| mk_heavy(i),
            |v, i, j, o| splice_move(v, i, j, o),
        )
    }

    // === std functions rewritten on the natives in later configurations ===

    public fun bench_std_take_u8() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u8(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.take(SIZE_PRIM / 2);
            assert!(out.length() == SIZE_PRIM / 2, 110);
            r = r + 1;
        };
    }

    public fun bench_std_take_u64() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u64(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.take(SIZE_PRIM / 2);
            assert!(out.length() == SIZE_PRIM / 2, 110);
            r = r + 1;
        };
    }

    public fun bench_std_take_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let out = base.take(SIZE_BOX / 2);
            assert!(out.length() == SIZE_BOX / 2, 110);
            r = r + 1;
        };
    }

    public fun bench_std_skip_u8() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u8(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.skip(SIZE_PRIM / 2);
            assert!(out.length() == SIZE_PRIM - SIZE_PRIM / 2, 111);
            r = r + 1;
        };
    }

    public fun bench_std_skip_u64() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u64(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let out = base.skip(SIZE_PRIM / 2);
            assert!(out.length() == SIZE_PRIM - SIZE_PRIM / 2, 111);
            r = r + 1;
        };
    }

    public fun bench_std_skip_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let out = base.skip(SIZE_BOX / 2);
            assert!(out.length() == SIZE_BOX - SIZE_BOX / 2, 111);
            r = r + 1;
        };
    }

    public fun bench_std_insert_u8() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u8(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let mut v = base;
            v.insert(mk_u8(r), SIZE_PRIM / 2);
            assert!(v.length() == SIZE_PRIM + 1, 112);
            r = r + 1;
        };
    }

    public fun bench_std_insert_u64() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u64(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let mut v = base;
            v.insert(mk_u64(r), SIZE_PRIM / 2);
            assert!(v.length() == SIZE_PRIM + 1, 112);
            r = r + 1;
        };
    }

    public fun bench_std_insert_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let mut v = base;
            v.insert(mk_heavy(r), SIZE_BOX / 2);
            assert!(v.length() == SIZE_BOX + 1, 112);
            r = r + 1;
        };
    }

    public fun bench_std_remove_u8() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u8(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let mut v = base;
            let _e = v.remove(SIZE_PRIM / 2);
            assert!(v.length() == SIZE_PRIM - 1, 113);
            r = r + 1;
        };
    }

    public fun bench_std_remove_u64() {
        let base = make_vec!(SIZE_PRIM, |i| mk_u64(i));
        let mut r = 0;
        while (r < LOOPS_PRIM) {
            let mut v = base;
            let _e = v.remove(SIZE_PRIM / 2);
            assert!(v.length() == SIZE_PRIM - 1, 113);
            r = r + 1;
        };
    }

    public fun bench_std_remove_heavy() {
        let base = make_vec!(SIZE_BOX, |i| mk_heavy(i));
        let mut r = 0;
        while (r < LOOPS_BOX) {
            let mut v = base;
            let _e = v.remove(SIZE_BOX / 2);
            assert!(v.length() == SIZE_BOX - 1, 113);
            r = r + 1;
        };
    }

    // === setup-only twins ===

    public fun bench_setup_copy1_u8() {
        run_copy1!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i))
    }

    public fun bench_setup_copy1_u64() {
        run_copy1!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i))
    }

    public fun bench_setup_copy1_u128() {
        run_copy1!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i))
    }

    public fun bench_setup_copy1_u256() {
        run_copy1!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i))
    }

    public fun bench_setup_copy1_address() {
        run_copy1!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i))
    }

    public fun bench_setup_copy1_light() {
        run_copy1!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i))
    }

    public fun bench_setup_copy1_heavy() {
        run_copy1!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i))
    }

    public fun bench_setup_copy2_u8() {
        run_copy2!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i))
    }

    public fun bench_setup_copy2_u64() {
        run_copy2!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i))
    }

    public fun bench_setup_copy2_u128() {
        run_copy2!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i))
    }

    public fun bench_setup_copy2_u256() {
        run_copy2!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i))
    }

    public fun bench_setup_copy2_address() {
        run_copy2!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i))
    }

    public fun bench_setup_copy2_light() {
        run_copy2!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i))
    }

    public fun bench_setup_copy2_heavy() {
        run_copy2!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i))
    }

    public fun bench_setup_noop_u8() {
        run_noop!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u8(i))
    }

    public fun bench_setup_noop_u64() {
        run_noop!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u64(i))
    }

    public fun bench_setup_noop_u128() {
        run_noop!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u128(i))
    }

    public fun bench_setup_noop_u256() {
        run_noop!(SIZE_PRIM, LOOPS_PRIM, |i| mk_u256(i))
    }

    public fun bench_setup_noop_address() {
        run_noop!(SIZE_PRIM, LOOPS_PRIM, |i| mk_address(i))
    }

    public fun bench_setup_noop_light() {
        run_noop!(SIZE_BOX, LOOPS_BOX, |i| mk_light(i))
    }

    public fun bench_setup_noop_heavy() {
        run_noop!(SIZE_BOX, LOOPS_BOX, |i| mk_heavy(i))
    }
}
