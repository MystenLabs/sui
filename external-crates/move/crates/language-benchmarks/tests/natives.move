module 0x1::bench {

    //
    // Global helpers
    //
    fun check(check: bool, code: u64) {
        if (check) () else abort code
    }

    //
    // `natives` benchmark
    //
    fun test_vector_ops<T>(mut x1: T, mut x2: T): (T, T) {
        let mut v: vector<T> = vector::empty();
        check(v.length() == 0, 100);
        v.push_back(x1);
        check(v.length() == 1, 101);
        v.push_back(x2);
        check(v.length() == 2, 102);
        v.swap(0, 1);
        x1 = v.pop_back();
        check(v.length() == 1, 103);
        x2 = v.pop_back();
        check(v.length() == 0, 104);
        v.destroy_empty();
        (x1, x2)
    }

    fun test_vector() {
        test_vector_ops<u8>(1u8, 2u8);
        test_vector_ops<u64>(1u64, 2u64);
        test_vector_ops<u128>(1u128, 2u128);
        test_vector_ops<bool>(true, false);
        test_vector_ops<address>(@0x1, @0x2);
        test_vector_ops<vector<u8>>(vector::empty(), vector::empty());
    }

    public fun bench() {
        let mut i = 0;
        // 300 is the number of loops to make the benchmark run for a couple of minutes,
        // which is an eternity.
        // Adjust according to your needs, it's just a reference
        while (i < 300) {
            test_vector();
            i = i + 1;
        }
    }
}
