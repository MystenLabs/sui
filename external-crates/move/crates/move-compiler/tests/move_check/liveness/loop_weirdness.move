#[allow(unused_trailing_semi)]
module 0x8675309::M {
    fun t() {
        let x = 0u64;
        let t = 1u64;

        if (x >= 0) {
            loop {
                let my_local = 0;
                if (my_local >= 0u64) { break; };
            };
            x = 1
        };
        t;
        x;
    }
}
