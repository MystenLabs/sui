// there is a parsing error and the following program text does not contain a token (due to missing
// quote) but the following function should still parse (fail during typing)
module 0x42::M {
    public fun missing_quote() {
        x"abcd
    }

    public fun wrong_return(): u64 {
    }
}
