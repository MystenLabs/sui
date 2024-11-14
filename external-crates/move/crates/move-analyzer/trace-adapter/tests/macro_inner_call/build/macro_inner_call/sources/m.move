// Test calling into another function inside a macro
// (stepping into a functino and braking in the function).
module macro_inner_call::m;

use macro_inner_call::m_dep::bar;

public fun foo(): u64 {
    let v = bar!(
        1,
        |x| x + x
    );
    bar!(v, |x| x + x)
}

#[test]
public fun test() {
    foo();
}
