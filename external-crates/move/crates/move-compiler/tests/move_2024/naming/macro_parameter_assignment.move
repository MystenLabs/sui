module a::m {
    macro fun call(f: |u64| -> u64, x: u64): u64 {
        f = abort 0;
        x = 0;
        f(x)
    }

    fun t() {
        // ensure the macro is expanded
        call!(|_| 1, 0) + 1;
    }
}
