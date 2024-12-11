module macro_inner_call::m_dep_dep;

public fun baz(p: u64): u64 {
    p + p
}
