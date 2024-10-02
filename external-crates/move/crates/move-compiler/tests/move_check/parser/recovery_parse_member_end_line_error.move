// there is a parsing error at the end of the line but the following function should still parse
// (fail during typing)
module 0x42::M {
    public fun foo

    public fun wrong_return(): u64 {
    }
}
