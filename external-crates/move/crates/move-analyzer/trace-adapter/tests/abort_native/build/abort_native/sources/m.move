// Test abort on an a native function when stepping out
// of a function containing the native call (before this
// call is stepped into or over).
module abort_native::m;

fun foo(v: vector<u8>, p: u64): (u64, address) {
    let val = p + p;
    let addr = sui::address::from_bytes(v);
    (val, addr)
}

#[test]
fun test() {
    foo(vector::singleton(42), 42);
}
