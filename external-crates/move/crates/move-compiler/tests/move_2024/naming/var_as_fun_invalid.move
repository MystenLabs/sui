module a::test_panic {
    public fun test_panic() {
        let var = 0;
        var();
    }
}
