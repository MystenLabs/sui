module 0x42::test {
    fun foo(i: u64) {
        assert!(i == 50, 0);
        let old_i = i;
        spec {
            assert old_i == 50;
        };
        while ({
            spec {
                invariant old_i <= i && i <= 100;
            };
            i < 100
        }) {
            spec {
                // fail to prove before the fix
                assert old_i == 50;
            };
            i = i + 1;
        };
        spec {
            assert i == 100;
        };
    }
}
