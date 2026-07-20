//# init --edition 2024.alpha

//# publish
module 0x42::a {
    public(package) const LIMIT: u64 = 10;

    public macro fun clamp($x: u64): u64 {
        let x = $x;
        if (x > LIMIT) LIMIT else x
    }
}

module 0x42::b {
    use 0x42::a;

    public fun check() {
        // the macro body's reference to LIMIT expands here, in another module, and calls the
        // getter synthesized in 0x42::a
        assert!(a::clamp!(4) == 4, 0);
        assert!(a::clamp!(11) == 10, 1);
    }
}

//# run 0x42::b::check
