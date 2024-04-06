module 0x42::m {

    public enum Box<T> { B { x: T } }

    fun test(opt: Box<u8>) {
        match (opt) {
            Box::B { x: _ } => (),
        }
    }

}
