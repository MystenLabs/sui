module macro_same_different_files2::m_dep;

public macro fun baz($p: u64): u64 {
    let ret = $p + $p;
    ret
}
