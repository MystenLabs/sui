module 0x2a::M {
    struct Box<T> has drop { f: T }
    fun f(): u64 {
        let b = Box { f: abort 0 };
        0
    }
}
