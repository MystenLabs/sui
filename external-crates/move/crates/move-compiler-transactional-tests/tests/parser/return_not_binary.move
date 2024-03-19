// Test returning expressions, that if parsed differently, could be binary operators,
// where the left argument would be the 'return'

//# publish
module 0x42::M {
    struct S { f: u64}

    fun t1(u: &u64): u64 {
        if (true) return * u;
        0
    }

    fun t2(s: &S): &u64 {
        if (true) return & s.f else & s.f
    }
}

//# run 0x42::M::t1 --args 0

//# run 0x42::M::t2 --args 0
