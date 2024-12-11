// suppress unnecessary_unit lint
module a::m {

    #[allow(lint(unnecessary_unit))]
    public fun test_empty_else(x: bool): bool {
        if (x) { x = true; } else {};
        if (!x) () else { test_empty_else(x); };
        { (); };
        ();
        x
    }
}
