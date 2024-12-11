module macro_inner_call::m_dep;

use macro_inner_call::m_dep_dep::baz;

public macro fun bar($param1: u64, $f: |u64| -> u64): u64 {
    let mut ret = baz($param1 + $param1);
    ret = ret + $f(ret);
    ret
}
