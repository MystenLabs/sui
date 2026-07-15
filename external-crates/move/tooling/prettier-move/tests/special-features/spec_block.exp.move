// options:
// printWidth: 80
// useModuleLabel: true

module prettier::spec_block;

fun add(a: u64, b: u64): u64 {
    a + b
}

spec add {
    ensures result == a + b;
    aborts_if a > 18446744073709551615 - b;
}

spec module {
    pragma verify = true;
    invariant forall i: u64 : i >= 0;
}

fun with_inline_spec(v: u64): u64 {
    spec {
        assume v > 0;
    };
    v
}
