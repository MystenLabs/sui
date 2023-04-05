#[test_only]
script {
    fun main() {}
}

#[test]
module 0x42::M {}

module 0x42::N {
    #[expected_failure(abort_code = 0)]
    struct S {}
}
