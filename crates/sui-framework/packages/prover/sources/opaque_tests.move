module prover::opaque_tests;

use prover::prover::{requires, ensures, asserts, old, max_u64};

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

fun scale<T>(r: &mut Range<T>, k: u64) {
    r.x = r.x * k;
    r.y = r.y * k;
}

// fun scale_spec<T>(r: &mut Range<T>, k: u64) {
//     let old_r = old!(r);

//     scale(r, k);

//     ensures(r.x == old_r.x * k);
//     ensures(r.y == old_r.y * k);
// }

fun scale_range<T, U>(r: &Range<T>, k: u64): Range<U> {
    let mut result = Range<U> { x: r.x, y: r.y };
    scale(&mut result, k);
    result
}