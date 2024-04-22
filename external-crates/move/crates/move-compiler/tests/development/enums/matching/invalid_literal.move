module 0x42::m {

    public enum Option<T> {
        Some(T)
    }

    fun test(opt: &Option<u8>) {
        match (opt) {
            Option::Some(256u8) => (),
        }
    }

}
