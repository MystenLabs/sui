module 0x42::m {

    fun x(): (u64, u64) { (0, 42) }

    fun test00() {
        match (x()) {
            (x, y) => ()
        }
    }

    fun test01() {
        match (x()) {
            _ => ()
        }
    }

}
