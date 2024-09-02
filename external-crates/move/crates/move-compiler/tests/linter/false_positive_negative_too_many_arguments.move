module 0x42::M {

    #[allow(unused_variable)]
    public fun false_positive(
        p1: u64, p2: u64, p3: u64, p4: u64, p5: u64,
        p6: u64, p7: u64, p8: u64, p9: u64, p10: u64
    ) {
        // Function body
    }

    // False Negative: Using a struct to bypass parameter limit
    #[allow(unused_variable)]
    struct ManyParams has drop {
        p1: u64, p2: u64, p3: u64, p4: u64, p5: u64,
        p6: u64, p7: u64, p8: u64, p9: u64, p10: u64,
        p11: u64
    }

    #[allow(unused_variable)]
    public fun false_negative(params: ManyParams) {
        // Function body
    }
}
