// options:
// printWidth: 80
// useModuleLabel: true

module prettier::comment_placement;

fun lets(c: bool): u64 {
    let a = // choose
    if (c) 1 else 2;
    let b = // call
    foo(1, 2);
    let d = // plain
        5;
    let e = // before equals
        6;
    a + b + d + e
}

fun unit_comments() {
    let u = ( /* unit */ );
    u
}

fun global_access() {
    ::sui::coin::zero();
    ::std::vector::empty<u64>();
}

fun dangling_in_call(x: u64) {
    foo(
        // dangling before a blank line
        x,
    );
    foo(/* one */ /* two */ x);
}
