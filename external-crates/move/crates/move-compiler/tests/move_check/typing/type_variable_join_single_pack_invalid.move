module 0x8675309::M {
    struct Box<T> has drop { f1: T, f2: T }

    fun t0() {
        let _b = Box { f1: false, f2: 1 };
        let _b2 = Box { f1: Box { f1: 0, f2: 0 }, f2:  Box { f1: false, f2: false } };
    }
}
