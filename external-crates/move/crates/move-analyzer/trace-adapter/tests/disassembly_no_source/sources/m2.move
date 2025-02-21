module disassembly_no_source::m2;

use disassembly_no_source::m3::bar;

public fun foo(p: u64): u64 {
    bar(p + p)
}

