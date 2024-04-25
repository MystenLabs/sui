module 0x42::m {

    public enum Box<T> { B { x: T } }

    fun test(b: &Box<u8>): &u8 {
        match (b) {
            Box::B { x: y } if (y == 0) => y,
            Box::B { x: y } => y,
        }
    }


}
