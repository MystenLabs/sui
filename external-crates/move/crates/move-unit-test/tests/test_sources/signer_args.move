module 0x6::M {

    #[test(_a=@0x1)]
    fun single_signer_pass(_a: signer) { }

    #[test(_a=@0x1)]
    fun single_signer_fail(_a: signer) {
        abort 0
    }

    #[test(_a=@0x1, _b=@0x2)]
    fun multi_signer_pass(_a: signer, _b: signer) { }

    #[test(_a=@0x1, _b=@0x2), expected_failure]
    fun multi_signer_fail(_a: signer, _b: signer) { }

    #[test(_a=@0x1, _b=@0x2), expected_failure]
    fun multi_signer_pass_expected_failure(_a: signer, _b: signer) {
            abort 0
    }

    #[test(a=@0x1, b=@0x2)]
    fun test_correct_signer_arg_addrs(a: signer, b: signer) {
        assert!(a != b)
    }
}
