// Test line breakpoints:
// - setting valid and invalid breakpoints
// - break at a breakpoint in the callee
// - break at a breakpoint after the loop
// - break at the breakpoint in the loop
module breakpoints_line::m;

fun foo(p: u64): u64 {
    let mut res = if (p < 1) {
        p + p
    } else {
        p + 1
    };

    while (res < 10) {
        res = res + 1;
    };
    res = res + p;
    while (res < 16) {
        res = res + 1;
    };
    res
}

#[test]
fun test() {
    let mut _res = foo(1);
    _res = _res + foo(_res);
    _res = _res + foo(_res); // to force another unoptimized read to keep `res` visible
}
