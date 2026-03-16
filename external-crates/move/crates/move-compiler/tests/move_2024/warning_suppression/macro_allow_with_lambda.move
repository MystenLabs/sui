// Tests that #[allow(...)] on a macro works when the macro takes lambda arguments.

module a::m {
    #[allow(unused_variable)]
    macro fun apply($f: |u64| -> u64): u64 {
        let unused = 0u64;
        let x = 10u64;
        $f(x)
    }

    fun call_it(): u64 {
        apply!(|x| x + 1)
    }
}
