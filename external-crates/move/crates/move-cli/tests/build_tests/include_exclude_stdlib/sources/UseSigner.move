module 0x1::Example {
    use std::address;

    public fun f(): u64 {
        address::length()
    }
}
