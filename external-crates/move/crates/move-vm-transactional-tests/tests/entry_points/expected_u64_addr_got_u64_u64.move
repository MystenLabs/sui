//# run --args 42 42
// should fail for mismatched types
module 0x42::m {
fun main(_x: u64, _y: address) {}
}
