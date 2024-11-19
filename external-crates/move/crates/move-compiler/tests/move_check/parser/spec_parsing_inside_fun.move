module 0x8675309::M {
    fun specs_in_fun(x: u64, n: u64) {
        // an ordinary assume
        spec_block;

        // loop invariant is written in a spec block inside the loop condition
        while ({
            spec_block;
            n < 64
        }) {
            spec_block;
            n = n + 1
        };

        // an ordinary assert
        spec_block;

        // loop invariant is written in a spec block at the beginning of loop body
        loop {
            spec_block;
            n = n + 1
        };

        // the following should parse successfully but fail typing
        spec {} + 1;
        spec {} && spec {};
        &mut spec_block;
    }
}
