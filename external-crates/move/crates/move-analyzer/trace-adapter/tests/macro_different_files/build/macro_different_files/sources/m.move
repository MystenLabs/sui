// Test stepping through macro defined different files.
module macro_different_files::m;

use macro_different_files::m_dep::bar;

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
