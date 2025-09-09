// tests that type annotations are persisted on lambdas when passing from one macro to another

module a::m;

macro fun do_u32($f: |u32| -> u64, $arg: _): u64 {
    $f($arg)
}

macro fun do_u16($f: |u16| -> u64, $arg: _): u64 {
    do_u32!($f, $arg)
}

fun t() {
    do_u16!(|x| x as u64, 0xFFFF_FFFF);
}


macro fun do2($f: |u32| -> u64, $g: |u16| -> u64): u64 {
    $f(0) + $g(0)
}

macro fun double($f: |u16| -> u64): u64 {
    do2!($f, $f)
}

fun t2() {
    double!(|x| x as u64);
}

macro fun do2_invalid($f: |u32| -> u64, $g: |u16| -> u64): u64 {
    $f(0xFFFF_FFFF) + $g(0xFFFF_FFFF)
}

macro fun double_under($f: |_| -> u64): u64 {
    do2_invalid!($f, $f)
}

fun t3() {
    double_under!(|x| x as u64);
}
