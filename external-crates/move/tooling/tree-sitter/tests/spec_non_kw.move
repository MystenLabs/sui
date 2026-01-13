module a::m;

public fun example(spec: u64) {
    call(spec);
    call(forall);
    call(exists);
}

fun call(spec: u64): u64 {
    let x = spec;
    x + x
}
