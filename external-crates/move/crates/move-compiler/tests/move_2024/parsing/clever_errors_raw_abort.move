#[allow(dead_code)]
module 0x42::a;

fun f() {
    abort
}

fun f1(): u64 {
    abort;
    1 + 1
}

fun f2(): u64 {
    1 + 2;
    abort;
    1 + 1
}

fun f3(): u64 {
    1 + abort;
    1 + 1
}

fun f4(): u64 {
    abort abort;
    1 + 1
}

#[allow(unused_trailing_semi)]
fun f5() {
    abort;
}

fun f6() {
    assert!(abort);
}

fun f7(v: u64) {
    if (v > 100) abort
}
