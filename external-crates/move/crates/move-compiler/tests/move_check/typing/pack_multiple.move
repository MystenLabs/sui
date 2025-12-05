module 0x8675309::M {
    struct Box<T> has drop { f: T }

    fun t0() {
        Box { f: (0u64, 1u64) };
        Box { f: (0u64, 1u64, 2u64) };
        Box { f: (true, Box { f: 0u64 }) };
    }
}
