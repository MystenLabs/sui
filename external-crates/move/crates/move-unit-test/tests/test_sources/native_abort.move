module 0x1::A {
    use std::vector;

    #[test]
    fun native_abort_unexpected_abort() {
        vector::borrow(&vector::empty<u64>(), 1);
    }

    #[test]
    #[expected_failure(vector_error, minor_status=0, location=0x1::A)]
    fun native_abort_good_wrong_code() {
        vector::borrow(&vector::empty<u64>(), 1);
    }

    #[test]
    #[expected_failure(vector_error, minor_status=1, location=0x1::A)]
    fun native_abort_good_right_code() {
        vector::borrow(&vector::empty<u64>(), 1);
    }
}
