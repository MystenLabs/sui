module 0x42::m {

    public enum Entry {
        One(u64)
    }

    fun main(e: &Entry) {
        match (e) {
            Entry::One { x } => ()
        }
    }

}
