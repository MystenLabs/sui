module 0x1::differential_test;

#[test]
fun f1() {
    f(0);
}

#[test]
fun f21() {
    f(21);
}

public fun f(x: u64): u64 {
    let x = x + 1;
    call(x);
    if (x > 10) {
        1
    } else {
        0
    }
}


public fun call(x: u64) {
    x + 1;
}
