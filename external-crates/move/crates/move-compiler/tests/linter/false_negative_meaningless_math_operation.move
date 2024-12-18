#[allow(lint(always_equal_operands))]
module 0x42::M {
    public fun zero_shift_complex(x: u64, y: u64): u64 {
        x * (y - y) // This is effectively * 0, but is not currently caught
    }

    public fun ast_fold() {
        // we do not lint on these because they are folded to a single value
        let x = 0;
        x * 1;
        1 * 0;
    }
}
