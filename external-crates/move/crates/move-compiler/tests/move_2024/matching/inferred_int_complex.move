module a::m;

fun t0() {
    let x = 2 + 5u64;
    match (x) { _ => {} }
}

fun t1() {
    match ({ 2 + 3u64 + 4}) { _ => {} }
}

fun t2() {
    match ({ let x = 2 + 3u64; x + 4}) { _ => {} }
}
