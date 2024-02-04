// there is a parsing error mid-line but the following function should still parse (fail during
// typing)
module 0x42::M {
    public fun () foo

    public fun wrong_return(): u64 {
    }
}
