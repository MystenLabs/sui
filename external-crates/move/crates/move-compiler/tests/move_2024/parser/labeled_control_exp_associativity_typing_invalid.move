// tests that control structures are right associative when not immediately followed by a block

// these cases do not type check

module 0x42::M {
    fun foo() {}
    fun bar(): u64 { 0 }

    fun t(cond: bool) {
        // if (cond) (bar() + 1);
        // so error about incompatible branches
        if (cond) bar() + 1;
        // (if (cond) bar()) + 1;
        // so error about wrong argument to +
        if (cond) 'a: { foo() } + 1;

        // while (cond) (bar() + 1);
        // so error about invalid loop body type
        'a: while (cond) bar() + 2;
        // ('a: while (cond) foo()) + 2
        // so error about wrong argument to +
        'a: while (cond) { foo() } + 2;
        while (cond) 'a: { return 'a foo() } + 2;

        // loop (bar() + 1);
        // so error about invalid loop body type
        'a: loop bar() + 2;
        // 'a: loop { foo() } + 2; would type check


        // does not type check since this return should be a break
        let _: u64 = loop 'a: { return 'a 0 } + 1;
    }
}
