// tests that control structures are right associative when not immediately followed by a block

// these cases type check, but have dead code

module 0x42::M {
    fun foo() {}
    fun bar(): u64 { 0 }

    fun t(cond: bool): u64 {
        // loop
        1 + loop { foo() } + 2;
        1 + loop foo();
        loop { foo() } + 1;

        // return
        return 1 + 2;
        return { 1 + 2 };
        return { 1 } && false;

        // abort
        abort 1 + 2;
        abort { 1 + 2 };
        abort { 1 } && false;

        0
    }
}
