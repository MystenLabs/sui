// tests that type annotations are persisted on lambdas when passing from one macro to another
// ensures that _ is instantiated separately each time

module a::m;

macro fun do2($f: |u32| -> u64, $g: |u16| -> u64): u64 {
    $f(0xFFFF_FFFF) + $g(0xFFFF)
}

macro fun double($f: |_| -> u64): u64 {
    do2!($f, $f)
}

fun t2() {
    double!(|x| x as u64);
}
