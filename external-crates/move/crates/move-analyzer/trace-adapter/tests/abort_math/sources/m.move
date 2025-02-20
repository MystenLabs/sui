// Test abort on an invalid math operation when continuing
// the end of the program.
module abort_math::m;

fun foo(p: u64): u64 {
    let val = p - 43;
    val
}

#[test]
fun test() {
    foo(42);
}
