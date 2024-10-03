module prover::opaque_tests;

use prover::prover::{requires, ensures, asserts, old, max_u64, fresh};

fun inc(x: u64): u64 {
    x + 1
}

fun inc_spec(x: u64): u64 {
    asserts((x as u128) + 1 <= max_u64() as u128);

    let result = inc(x);

    ensures(result == x + 1);

    result
}

fun add(x: u64, y: u64): u64 {
    x + y
}

fun add_spec(x: u64, y: u64): u64 {
    asserts((x as u128) + (y as u128) <= max_u64() as u128);

    let result = add(x, y);

    ensures(result == x + y);

    result
}

fun double(x: u64): u64 {
    add(x, x)
}

fun double_spec(x: u64): u64 {
    asserts((x as u128) * 2 <= max_u64() as u128);

    let result = double(x);

    ensures(result == x * 2);

    result
}

fun add_wrap(x: u64, y: u64): u64 {
    (((x as u128) + (y as u128)) % 18446744073709551616) as u64
}

fun add_wrap_spec(x: u64, y: u64): u64 {
    let result = add_wrap(x, y);
    ensures(result == x.to_int().add(y.to_int()).to_u64());
    result
}

fun double_wrap(x: u64): u64 {
    add_wrap(x, x)
}

fun double_wrap_spec(x: u64): u64 {
    let result = double_wrap(x);
    ensures(result == x.to_int().mul((2 as u8).to_int()).to_u64());
    result
}

fun add_wrap_buggy(x: u64, y: u64): u64 {
    x + y
}

fun add_wrap_buggy_no_verify_spec(x: u64, y: u64): u64 {
    let result = add_wrap_buggy(x, y);
    ensures(result == x.to_int().add(y.to_int()).to_u64());
    result
}

fun double_wrap_buggy(x: u64): u64 {
    add_wrap_buggy(x, x)
}

fun double_wrap_buggy_spec(x: u64): u64 {
    let result = double_wrap_buggy(x);
    ensures(result == x.to_int().mul((2 as u8).to_int()).to_u64());
    result
}

public struct Range<phantom T> {
    x: u64,
    y: u64,
}

fun size<T>(r: &Range<T>): u64 {
    r.y - r.x
}

fun size_spec<T>(r: &Range<T>): u64 {
    requires(r.x <= r.y);

    let result = size(r);

    ensures(result == r.y - r.x);

    result
}

fun add_size<T, U>(r1: &Range<T>, r2: &Range<U>): u64 {
    size(r1) + size(r2)
}

fun add_size_spec<T, U>(r1: &Range<T>, r2: &Range<U>): u64 {
    requires(r1.x <= r1.y);
    requires(r2.x <= r2.y);

    asserts(((r1.y - r1.x) as u128) + ((r2.y - r2.x) as u128) <= max_u64() as u128);

    let result0 = add_size(r1, r2);

    ensures(result0 == (r1.y - r1.x) + (r2.y - r2.x));

    result0
}

fun scale<T>(r: &mut Range<T>, k: u64) {
    r.x = r.x * k;
    r.y = r.y * k;
}

fun scale_spec<T>(r: &mut Range<T>, k: u64) {
    let old_r = old!(r);

    requires(r.x <= r.y);

    asserts(r.y.to_int().mul(k.to_int()).lte(max_u64().to_int()));

    scale(r, k);

    ensures(r.x == old_r.x * k);
    ensures(r.y == old_r.y * k);
}

fun fresh_with_type_withness<T, U>(_: &T): U {
    fresh()
}

fun fresh_with_type_withness_spec<T, U>(x: &T): U {
    fresh_with_type_withness(x)
}
