module a::m;

fun t0() {
    let x = 2 + 5;
    match (x) { _ => {} }
}

fun t1() {
    match ({ 2 + 3 + 4}) { _ => {} }
}

fun t2() {
    match ({ let x = 2 + 3; x + 4}) { _ => {} }
}
