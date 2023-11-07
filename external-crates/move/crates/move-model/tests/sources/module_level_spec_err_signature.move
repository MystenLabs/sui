module 0x42::TestModule {

    struct R has key { value: u64 }

    fun store(_s: &signer, _value: u64) {

    }
}

spec 0x42::TestModule {
    spec store(s: &signer) {
    }

    spec store_undefined(s: &signer, value: u64) {
    }
}
