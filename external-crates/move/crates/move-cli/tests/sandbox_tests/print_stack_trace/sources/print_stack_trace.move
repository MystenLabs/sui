module 0x42::print_stack_trace {
    use std::debug;
    use std::vector;
    use 0x2::N;

    #[allow(unused_mut_ref)]
    entry fun print_stack_trace() {
        let v = vector::empty();
        vector::push_back(&mut v, true);
        vector::push_back(&mut v, false);
        let r = vector::borrow(&mut v, 1);
        let x = N::foo<bool, u64>();
        debug::print(&x);
        _ = r;
        N::foo<u8,bool>();
    }
}
