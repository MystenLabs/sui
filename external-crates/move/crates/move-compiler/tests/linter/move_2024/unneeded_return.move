module 0x42::m;

fun t0(): u64 { return 5 }

fun t1(): u64 { return t0() }

public struct S {  }

fun t2(): S { return S { } }

public enum E { V }

fun t3(): E { return E::V }

fun t4(): vector<u64> { return vector[1,2,3] }

fun t5() { return () }

fun t6(): u64 { let x = 0; return move x
}

fun t7(): u64 {
    let x = 0;
    return copy x
}

const VALUE: u64 = 0;

fun t8(): u64 { return VALUE }

fun t9(): &u64 {
    let x = 0;
    return &x
}

fun t10(): u64 { return 5 + 2 }

fun t11(): bool { return !true }

fun t12(x: &u64): u64 { return *x }

fun t13(x: u64): u128 { return x as u128 }

fun t14(): u64 { return (0: u64) }

fun t15(): u64 { return loop { break 5 } }
