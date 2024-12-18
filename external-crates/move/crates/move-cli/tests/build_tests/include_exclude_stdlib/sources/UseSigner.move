module 0x6::Example {
    use std::address;

    public fun f(): u64 {
        address::length()
    }
}
