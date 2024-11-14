// Test calling into another macro inside a macro
// (stepping into the inner macro and braking in the inner macro).
module macro_inner_macro::m;

use macro_inner_macro::m_dep::bar;

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
