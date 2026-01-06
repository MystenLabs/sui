module a::m;

fun t() {
    match (&mut 10u64) { _ => {} }
}
