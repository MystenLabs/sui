address 0x2 {
module M {
    use std::debug;

    fun f() {
        debug::print(&7);
    }
}
}
