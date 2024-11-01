module 0x42::m;

fun t0() { return abort 0 }

// invalid type, so no lint
fun t1() {
    let mut x = 0;
    return (x = 1)
}

// invalid syntax, so no lint
fun t2() {
    let mut x = 0;
    let y = &mut x;
    return (*x = 1)
}

// invalid type, so no lint
fun t3() { 'a: { return { return 'a 0 } } }

// the following are all invalid syntax, so no lint

fun t4() { return continue }

fun t5() { return break }

fun t6() { return (break + 5) }

fun t7() { return unnamed_fun() }

fun t8(cond: bool) { return while (cond) { break } }
