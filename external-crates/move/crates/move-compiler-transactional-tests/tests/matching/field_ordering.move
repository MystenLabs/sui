//# init --edition 2024.beta

//# publish
module 0x42::m;

public enum E {
    V { zero: u64, one: u64 }
}

fun add1(n: u64): u64 { n + 1 }

public fun main() {
    let one = match (E::V { zero: 0, one: 1}) {
        E::V { one: _, zero: one } => add1(one)
    };
    assert!(one == 1)
}

//# run 0x42::m::main
