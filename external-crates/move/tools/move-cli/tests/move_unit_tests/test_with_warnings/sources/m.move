module 0x42::m {
public fun foo(x: u64): u64 {
    1 + 1
}

#[test]
fun nop() {}

#[test]
#[expected_failure]
fun explicit_abort_expect_failure() {
    abort 42
}
}
