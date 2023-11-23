module 0x42::m {
    use std::vector;
    entry fun bad_borrow() {
        let v = vector::empty<bool>();
        let _ref = vector::borrow(&v, 0);
    }
}
