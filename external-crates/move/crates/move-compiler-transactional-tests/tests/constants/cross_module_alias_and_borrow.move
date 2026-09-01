//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const MAX: u64 = 100;
    public(package) const BYTES: vector<u8> = b"hi";
}

module 0x42::b {
    use 0x42::a;
    use 0x42::a::MAX;

    const D: u64 = MAX + 1;

    // the warning itself is covered by the move_2024/constants/cross_module_borrow test; its
    // output embeds a temp path, which cannot be snapshotted here
    #[allow(implicit_const_copy)]
    public fun check() {
        // through a member 'use' alias, in a function body and a constant definition
        assert!(MAX == 100, 0);
        assert!(D == 101, 1);
        // borrows of cross-module constants
        assert!(*&a::MAX == 100, 2);
        assert!(*&a::BYTES == b"hi", 3);
    }
}

//# run 0x42::b::check
