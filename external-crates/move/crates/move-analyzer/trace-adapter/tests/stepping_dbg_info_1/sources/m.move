// Same as stepping test but source maps are replaced
// with the first version of debug info.
module stepping_dbg_info_1::m;

fun foo(p: u64): u64 {
    p + p
}

#[test]
fun test() {
    let mut _res = foo(42);
    _res = _res + foo(_res);
    _res = _res + foo(_res); // to force another unoptimized read to keep `res` visible
}
