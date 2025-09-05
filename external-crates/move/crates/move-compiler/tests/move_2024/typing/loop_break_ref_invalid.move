module a::m;

fun test(a: &u64): &mut u64 {
    let x = loop {
        break a
    };
    x
}
