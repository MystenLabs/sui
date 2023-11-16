module 0x42::TestModule {

    struct R has key { value: u64 }

    fun store(_s: &address, _value: u64) {

    }
}

spec 0x42::TestModule {
    spec store(s: &address) {
    }

    spec store_undefined(s: &address, value: u64) {
    }
}
