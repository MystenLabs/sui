module a::m;


fun t<T>(x: T): T {
    x
}

fun test() {
    let mut x = 1;
    let y = &mut x;
    let z = t(y);
    assert!(*z == 1);
}
