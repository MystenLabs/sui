module 0x42::l {

    public fun a(x: u64): u64 { x + 1 }

}

module 0x42::m {
    use 0x42::l::Self;

    public enum Option<T> {
        Some(T)
    }

    fun test(opt: &Option<u8>) {
        match (opt) {
            l::a(_) => (),
        }
    }

}
