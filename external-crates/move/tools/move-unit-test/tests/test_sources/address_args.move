address 0x1 {
module M {
    const ErrorCode: u64 = 100;

    #[test(a = @0x42)]
    fun correct_address(a: address) {
        assert!(a == @0x42, 100);
    }

    #[test(a = @0x42, b = @0x43)]
    fun correct_addresses(a: address, b: address) {
        assert!(a == @0x42, 100);
        assert!(b == @0x43, 101);
    }

    #[test(a = @0x42)]
    #[expected_failure(abort_code = ErrorCode)]
    fun wrong_address(a: address) {
        assert!(a == @0x43, 100);
    }
}
}
