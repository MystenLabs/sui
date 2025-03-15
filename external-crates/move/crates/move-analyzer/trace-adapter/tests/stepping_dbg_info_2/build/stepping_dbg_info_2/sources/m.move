// Same as stepping test but with the second
// version of debug info format (mostly
// to make sure that it's injested correctly
// if the extra `from_file_path` field is present
// though in this test it's either `null` or
// represents a non-existent path as encoding
// absolute path to work in CI would be brittle).
module stepping_dbg_info_2::m;

fun foo(p: u64): u64 {
    p + p
}

#[test]
fun test() {
    let mut _res = foo(42);
    _res = _res + foo(_res);
    _res = _res + foo(_res); // to force another unoptimized read to keep `res` visible
}
