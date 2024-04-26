// there is a parsing error mid-module but the following module should still parse (fail during
// typing)
module 0x42::M1 {
    public fun () foo

    public fun bar() {
    }
}

module 0x42::M2 {
    public fun wrong_return(): u64 {
    }
}
