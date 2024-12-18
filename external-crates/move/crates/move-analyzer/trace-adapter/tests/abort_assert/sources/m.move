// Test abort on an assertion when stepping (but not stepping over).
module abort_assert::m;

fun foo(p: u64): u64 {
    let val = p + p;
    assert!(val != 84);
    val
}

#[test]
fun test() {
    foo(42);
}
