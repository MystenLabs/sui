address 0x1 {
module M {
    #[test]
    #[expected_failure]
    fun u64_sub_underflow() {
        0 - 1;
    }

    #[test]
    #[expected_failure]
    fun u64_add_overflow() {
        18446744073709551615 + 1;
    }

    #[test]
    #[expected_failure]
    fun u64_div_by_zero() {
        1/0;
    }

    #[test]
    #[expected_failure]
    fun u64_mul_overflow() {
        4294967296 * 4294967296;
    }
}
}
