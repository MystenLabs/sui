// check that only non-annotated integer literals and u64s can be assigned to
// abort codes
address 0x1 {
module M {
    #[test]
    #[expected_failure(abort_code=0, location=0x1::M)]
    fun ok1() { }

    #[test]
    #[expected_failure(abort_code=0u64, location=0x1::M)]
    fun ok2() { }

    #[test]
    #[expected_failure(abort_code=0u8, location=0x1::M)]
    fun fail_annot1() { }

    #[test]
    #[expected_failure(abort_code=0u128, location=0x1::M)]
    fun fail_annot3() { }
}
}
