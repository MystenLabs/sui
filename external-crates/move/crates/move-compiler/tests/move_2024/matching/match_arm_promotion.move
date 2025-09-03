module a::m;

fun test(x: &u64, y: &mut u64): &u64 {
    match (true) {
        true => x,
        false => y,
    }
}
