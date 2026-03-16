// Tests edge cases for the pre-expansion unused_let_mut analysis on macro bodies.
//
// The analysis runs at the naming level (before type-checking), so it can't
// detect mutation through method calls with &mut self — it conservatively treats
// all method receivers as potentially mutated.

module a::m {

    public struct S has copy, drop { value: u64 }

    // --- Cases that should NOT warn ---

    // Mutation via direct assignment.
    macro fun assign_mut(): u64 {
        let mut x = 0u64;
        x = 1;
        x
    }

    // Mutation via field assignment.
    macro fun field_assign_mut(): S {
        let mut s = S { value: 0 };
        s.value = 42;
        s
    }

    // Mutation via method call (conservative: any method call on the variable
    // is assumed to potentially mutate it since we lack type info at naming).
    macro fun method_call_mut(): vector<u64> {
        let mut v = vector[];
        v.push_back(1);
        v
    }

    // Mutation via explicit &mut borrow.
    macro fun explicit_borrow_mut(): u64 {
        let mut x = 0u64;
        give_me_mut(&mut x);
        x
    }

    // Mutation inside a nested block.
    macro fun nested_block_mut(): u64 {
        let mut x = 0u64;
        {
            x = 42;
        };
        x
    }

    // Mutation inside a while loop body.
    macro fun loop_body_mut(): u64 {
        let mut x = 0u64;
        let mut i = 0u64;
        while (i < 10) {
            x = x + 1;
            i = i + 1;
        };
        x
    }

    // Mutation inside if-else branches.
    macro fun if_else_mut($cond: bool): u64 {
        let mut x = 0u64;
        if ($cond) {
            x = 1;
        } else {
            x = 2;
        };
        x
    }

    // Mutation via destructuring assignment.
    macro fun destructure_mut(): u64 {
        let mut a = 0u64;
        let mut b = 0u64;
        (a, b) = (1, 2);
        a + b
    }

    // Underscore-prefixed mut variable: no warning (skipped by convention).
    macro fun underscore_mut(): u64 {
        let mut _x = 0u64;
        42
    }

    // --- Cases that SHOULD warn ---

    // Simple case: never mutated, just read.
    macro fun simple_read_only(): u64 {
        let mut x = 5u64;
        x + 1
    }

    // Multiple bindings: one mutated, one not.
    macro fun mixed_mut(): u64 {
        let mut used = 0u64;
        let mut unused = 0u64;
        used = 42;
        used + unused
    }

    // Helpers
    fun give_me_mut(_x: &mut u64) {}

    fun call_them() {
        assign_mut!();
        field_assign_mut!();
        method_call_mut!();
        explicit_borrow_mut!();
        nested_block_mut!();
        loop_body_mut!();
        if_else_mut!(true);
        destructure_mut!();
        underscore_mut!();
        simple_read_only!();
        mixed_mut!();
    }
}
