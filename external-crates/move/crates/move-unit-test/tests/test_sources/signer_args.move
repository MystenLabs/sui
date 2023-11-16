address 0x1 {
module M {

    #[test(_a=@0x1)]
    fun single_signer_pass(_a: address) { }

    #[test(_a=@0x1)]
    fun single_signer_fail(_a: address) {
        abort 0
    }

    #[test(_a=@0x1, _b=@0x2)]
    fun multi_signer_pass(_a: address, _b: address) { }

    #[test(_a=@0x1, _b=@0x2), expected_failure]
    fun multi_signer_fail(_a: address, _b: address) { }

    #[test(_a=@0x1, _b=@0x2), expected_failure]
    fun multi_signer_pass_expected_failure(_a: address, _b: address) {
            abort 0
    }

    #[test(a=@0x1, b=@0x2)]
    fun test_correct_signer_arg_addrs(a: address, b: address) {
        assert!(a == @0x1, 0);
        assert!(b == @0x2, 1);
    }
}
}
