address 0x2 {
module M {
    use std::debug;

    #[allow(unused_function)]
    fun f() {
        debug::print(&7);
    }
}
}
