module macro_different_same_files::m_dep;

public macro fun baz($p: u64): u64 {
    let ret = $p + $p;
    ret
}


public macro fun bar($param1: u64, $f: |u64| -> u64): u64 {
    let mut ret = baz!($param1);
    ret = ret + $f(ret);
    ret
}
