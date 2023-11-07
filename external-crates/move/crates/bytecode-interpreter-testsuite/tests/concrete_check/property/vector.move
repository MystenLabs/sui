module 0x2::A {
    use std::vector;

    #[test]
    public fun check_vector() {
        let a = vector::empty();
        let b = vector::empty();
        spec {
            assert len(a) == 0;
            assert a == vec();
        };

        vector::push_back(&mut a, 42u128);
        vector::push_back(&mut b, 0u128);
        spec {
            assert len(a) == 1;
            assert a == vec(42);
            assert a[0] == 42;
            assert contains(a, 42);
            assert index_of(a, 42) == 0;
            assert update(b, 0, 42) == a;
            assert concat(a, b) == concat(vec(42), vec(0));
            assert in_range(a, 0);
            assert !in_range(b, 2);
        };
    }
}
