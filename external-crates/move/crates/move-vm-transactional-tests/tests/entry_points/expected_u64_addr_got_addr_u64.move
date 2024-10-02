//# run --args 0x1 42
// should fail, flipped arguments
module 0x42::m {
fun main(_x: u64, _y: address) {}
}
