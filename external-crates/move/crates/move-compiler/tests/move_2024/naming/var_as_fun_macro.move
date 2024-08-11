module a::test_panic {
    public macro fun test_panic($var: u64): u64 {
        $var();
        0
    }

    public fun t(): u64 {
        test_panic!(0)
    }
}
