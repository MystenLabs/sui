//# init --edition 2024.beta

//# publish
module 0x42::m;

fun add(x: u64, y: u64): u64 { x + y }

fun t(z: &mut u64): u64 {
    add(
        match (z) { 0 => 0, n => { *n = *n + 1; *n }},
        match (z) { 0 => 0, n => { *n = *n * *n; *n }}
    )
}

public fun test() {
    let mut a = 2;
    assert!(t(&mut a) == 12);
}

//# run 0x42::m::test
