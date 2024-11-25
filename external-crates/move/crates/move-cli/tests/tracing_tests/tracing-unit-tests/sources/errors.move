module 0x1::errors {
    #[test]
    #[expected_failure]
    fun aborter() {
        let x = 1 + 1;
        abort x
    }

    #[test]
    #[expected_failure]
    fun div_0() {
        1/0;
    }

    #[test]
    #[expected_failure]
    fun underflow() {
        1 - 10;
    }

    #[test]
    #[expected_failure]
    fun bad_cast() {
        256u64 as u8;
    }

    #[test]
    #[expected_failure]
    fun overshift_l() {
        1u8 << 20;
    }

    #[test]
    #[expected_failure]
    fun overshift_r() {
        1u8 << 20;
    }

    #[test]
    #[expected_failure]
    fun fail_during_abort() {
        abort 1/0
    }

    #[test]
    #[expected_failure]
    fun fail_in_native() {
        std::string::internal_sub_string_for_testing(&vector[1, 2, 3], 4, 1);
    }
}
