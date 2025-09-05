module a::m;

fun test(a: &mut u64): &u64 {
    let x = loop {
        break a
    };
    x
}
