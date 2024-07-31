module 0x42::m {
    entry fun bad_borrow() {
        let v = vector::empty<bool>();
        let _ref = vector::borrow(&v, 0);
    }
}
