// Test variable shadowing: creation and disposal of shadowed variables
// and scopes.
module shadowing::m;

fun foo(p: bool, val1: u64, val2: u64, shadowed_var: u64): u64 {
    let mut res = 0;

    if (p) {
        let shadowed_var = val1 + shadowed_var;
        if (shadowed_var < 42) {
            let shadowed_var = val2 + shadowed_var;
            if (shadowed_var < 42) {
                res = 42 + shadowed_var;
            };
            res = res + shadowed_var;
        };
        res = res + shadowed_var;
    };

    res + shadowed_var
}

#[test]
fun test() {
    foo(true, 7, 7, 7);
}
