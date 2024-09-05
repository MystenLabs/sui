module 0x42::m;
const ZERO: u64 = 0;

fun t0(cond: bool): u64 {
    if (cond) { return 5 } else { abort ZERO }
}

fun t1(cond: bool): u64 {
    if (cond) { return 5 } else { return 0 }
}

fun t2(cond: bool): u64 {
    return if (cond) { 5 } else { 0 }
}

fun t3(cond: bool): u64 {
    return if (cond) { return 5 } else { 0 }
}

fun t4(cond: bool): u64 {
    return if (cond) { 5 } else { return 0 }
}

public enum E has drop {
    V0,
    V1
}

fun t5(e: E): u64{
    return match (e) {
        E::V0 => 0,
        E::V1 => 1,
    }
}

fun t6(e: E): u64{
    match (e) {
        E::V0 => return 0,
        E::V1 => 1,
    }
}

fun t7(e: E): u64 {
    match (e) {
        E::V0 => return 0,
        E::V1 => return 1,
    }
}

fun t8(e: E): u64{
    match (e) {
        E::V0 => return 0,
        E::V1 => abort ZERO,
    }
}

fun t9(e: E): u64 {
    return match (e) {
        E::V0 => 0,
        E::V1 => abort ZERO,
    }
}

fun t10(e: E): u64 {
    return match (e) {
        E::V0 => if (true) { return 0 } else { 1 },
        E::V1 => 2,
    }
}
