// Test native function execution (vector length).
module native_fun::m;

use std::string::{String, utf8, index_of};

fun foo(s: String, sub: vector<u8>, p: u64): u64 {
    s.index_of(&utf8(sub)) + p
}

#[test]
fun test() {
    let mut _res = foo(utf8(b"hello"), b"e", 42);
    _res = _res + foo(utf8(b"hello"), b"l", _res); // to force another unoptimized read to keep `res` visible
}
