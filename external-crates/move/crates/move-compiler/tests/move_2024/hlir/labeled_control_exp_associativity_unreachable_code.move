// tests that control structures are right associative when not immediately followed by a block

// these cases type check, but have dead code

module 0x42::M {
    fun foo() {}
    fun bar(): u64 { 0 }

    fun t(): u64 { 'r: {
        // loop
        1 + 'a: loop { foo() } + 2;
        1 + 'a: loop foo();
        1 + loop 'a: { foo() } + 2;
        'a: loop { foo() } + 1;

        // return
        return 'r 1 + 2;
        return 'r { 1 + 2 };
        return 'r { 1 } && false;
        false && return 'r { 1 };

        // abort
        abort 1 + 2;
        abort 'a: { 1 + 2 };
        abort 'a: { 1 } && false;
        false && abort 'a: { 1 };

        0
    } }
}
