// options:
// printWidth: 80
// useModuleLabel: true
// enableErrorDebug: true

module prettier::spec_block_debug;

fun add(a: u64, b: u64): u64 {
    a + b
}

/* UNHANDLED: spec_block */ spec add {
    ensures result == a + b;
}
