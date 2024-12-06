// Test simple stepping functionality:
// - step into a function
// - step out of a function
// - step over a function
module stepping::m;

fun foo(p: u64): u64 {
    p + p
}

#[test]
fun test() {
    let mut _res = foo(42);
    _res = _res + foo(_res);
    _res = _res + foo(_res); // to force another unoptimized read to keep `res` visible
}
