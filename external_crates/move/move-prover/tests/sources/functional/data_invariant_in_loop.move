module 0x42::data_inv_in_loop {
    use std::option::{Self, Option};
    use std::vector;

    public fun test() {
        let i = 0;
        let r = vector::empty<Option<u64>>();
        while(i < 10) {
            vector::push_back(&mut r, option::none());
            i = i + 1;
        }
    }
}
