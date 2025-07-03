module a::m;

fun test(a: &u64): &mut u64 {
    loop {
        break a
    }
}
