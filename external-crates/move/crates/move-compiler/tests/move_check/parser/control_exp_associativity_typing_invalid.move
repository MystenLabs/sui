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
        if (cond) { foo() } + 1;

        // while (cond) (bar() + 1);
        // so error about invalid loop body type
        while (cond) bar() + 2;
        // (while (cond) foo()) + 2
        // so error about wrong argument to +
        while (cond) { foo() } + 2;

        // loop (bar() + 1);
        // so error about invalid loop body type
        loop bar() + 2;
        // loop { foo() } + 2; would type check
    }
}
