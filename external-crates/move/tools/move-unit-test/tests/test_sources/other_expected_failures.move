module 0x42::m {
    #[test]
    #[expected_failure(vector_error, minor_status=1, location=Self)]
    fun t0() {
        std::vector::borrow(&vector[0], 2);
    }

    #[test]
    #[expected_failure(arithmetic_error, location=Self)]
    fun t1() {
        1 / 0;
    }

    #[test]
    #[expected_failure(arithmetic_error, location=Self)]
    fun t2() {
        0 - 1;
    }

    #[test]
    #[expected_failure(arithmetic_error, location=Self)]
    fun t3() {
        1 % 0;
    }

    #[test]
    #[expected_failure(out_of_gas, location=Self)]
    fun t4() {
        loop {}
    }

    struct R has key { }

    #[test(a=@1)]
    #[expected_failure(major_status=4004, location=Self)]
    fun t5(a: signer) {
        move_to(&a, R {});
        move_to(&a, R {});
    }

}
