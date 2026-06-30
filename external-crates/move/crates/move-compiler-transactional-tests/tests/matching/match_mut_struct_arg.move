//# init --edition 2024.beta

//# publish
module 0x42::m;

public struct S has drop { n: u64 }

fun add(x: u64, y: u64): u64 { x + y }

fun t(z: &mut S): u64 {
    add(
        match (z) { S { n: 0 } => 0, S { n } => { *n = *n + 1; *n }},
        match (z) { S { n: 0 } => 0, S { n } => { *n = *n * *n; *n }}
    )
}

public fun test() {
    let mut s = S { n : 2 };
    assert!(t(&mut s) == 12);
}

//# run 0x42::m::test
