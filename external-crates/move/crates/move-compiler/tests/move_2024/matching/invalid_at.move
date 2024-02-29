module 0x42::m {

    public enum Option<T> {
        Some(T)
    }

    fun test(opt: &Option<u8>) {
        match (opt) {
            _ @ Option::Some(128u8) => (),
        }
    }

}
