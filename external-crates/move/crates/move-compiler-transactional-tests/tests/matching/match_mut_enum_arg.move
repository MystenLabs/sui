//# init --edition 2024.beta

//# publish
module 0x42::m;

public enum Maybe<T> has drop {
    Nothing,
    Just(T),
}

fun add(x: u64, y: u64): u64 { x + y }

fun t(z: &mut Maybe<u64>): u64 {
    add(
        match (z) { Maybe::Nothing => 0, Maybe::Just(n) => { *n = *n + 1; *n }},
        match (z) { Maybe::Nothing => 0, Maybe::Just(n) => { *n = *n * *n; *n }}
    )
}

public fun test() {
    let mut s = Maybe::Just(2);
    assert!(t(&mut s) == 12);
}

//# run 0x42::m::test
