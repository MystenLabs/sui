module a::m {
    // cases that need parens
    fun t(cond: bool): u64 {
        loop { break ('a: { 1u64 }) };
        loop { break ('a: loop { break 0u64 }) };
        if (cond) return ('a: { 1 });
        0
    }
    fun t2(cond: bool) {
        if (cond) return ('a: while (cond) {});
    }

    // misleading cases but valid
    fun t3(cond: bool) {
        'a: loop { break 'a { 1u64 } };
        'a: loop { break 'a loop { break 0u64 } };
        'a: { return 'a { 1u64 } };
        'a: { return 'a while (cond) {} };
    }
}
