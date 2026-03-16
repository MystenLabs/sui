// Tests that lambda arguments to macros do not produce spurious warnings.
//
// When a user passes a lambda to a macro, the lambda's parameters become
// macro-generated bindings (color > 0) after expansion. Warnings about those
// bindings are suppressed by is_from_macro_expansion() — the user should not
// be penalized for how the macro plumbs their lambda.

module a::m {
    // The macro takes a lambda and calls it. The lambda's parameter binding
    // becomes a macro-generated variable after expansion.
    macro fun apply($f: |u64| -> u64): u64 {
        $f(42u64)
    }

    // The lambda parameter `x` is used — no warning expected.
    fun lambda_used(): u64 {
        apply!(|x| x + 1)
    }

    // The lambda parameter `_x` starts with underscore — no warning expected.
    fun lambda_underscore(): u64 {
        apply!(|_x| 99u64)
    }

    // The macro takes a lambda with &mut parameter. The user's lambda doesn't
    // need to mutate via the reference — this should not produce unused_let_mut.
    macro fun with_mut_lambda($f: |&mut u64|) {
        let mut x = 0u64;
        $f(&mut x);
    }

    fun lambda_ignores_mut(): u64 {
        with_mut_lambda!(|_r| {});
        0
    }

    // Two-lambda macro: $g is unused in the macro body, so naming warns about the
    // unused parameter at the definition site. The caller also gets a dead_code
    // warning for the unused argument expression.
    macro fun two_lambdas($f: |u64| -> u64, $g: |u64| -> u64): u64 {
        $f(10u64)
    }

    fun only_first_used(): u64 {
        two_lambdas!(|x| x + 1, |y| y * 2)
    }
}
