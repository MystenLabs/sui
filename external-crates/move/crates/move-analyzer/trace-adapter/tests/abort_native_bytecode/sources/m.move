// Test abort on a native aborting function representing
// a bytecode, when stepping over a function containing
// the native call.
module abort_native_bytecode::m;

fun foo(v: vector<u64>): u64 {
    let val = v.borrow(0);
    *val
}

#[test]
fun test() {
    foo(vector::empty());
}
