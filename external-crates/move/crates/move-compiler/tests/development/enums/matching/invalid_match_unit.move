module 0x42::m {

    fun x() { }

    fun test00() {
        match (x()) {
            () => ()
        }
    }

    fun test01() {
        match (x()) {
            _ => ()
        }
    }

}
