module macro_breakpoint::m_dep;

public macro fun bar($param1: u64, $f: |u64| -> u64): u64 {
    let mut ret = $param1 + $param1;
    ret = ret + $f(ret);
    ret
}
