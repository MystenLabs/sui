module macro_inner_macro::m_dep_dep;

public macro fun baz($p: u64): u64 {
    let ret = $p + $p;
    ret
}
