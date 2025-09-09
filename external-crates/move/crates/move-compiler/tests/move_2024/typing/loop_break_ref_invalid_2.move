module a::m;

fun test(a: &u64): &mut u64 {
    let b = a;
    loop {
        break b
    }
}
