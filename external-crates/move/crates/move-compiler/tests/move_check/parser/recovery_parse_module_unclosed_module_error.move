// there is a parsing error at the end of first module which is not even closed with the curly, but
// the following module should still parse (fail during typing)
module 0x42::M1 {
    public fun


module 0x42::M2 {
    public fun wrong_return(): u64 {
    }
}
