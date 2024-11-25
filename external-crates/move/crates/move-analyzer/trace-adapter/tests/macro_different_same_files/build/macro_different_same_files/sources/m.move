// Test stepping through two macros, one defined in a different file,
// and one defined in the same file.
module macro_different_same_files::m;

use macro_different_same_files::m_dep::bar;

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
