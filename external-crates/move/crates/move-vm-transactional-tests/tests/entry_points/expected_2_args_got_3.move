//# run --args 42 0x1 42
// should fail, too many args
module 0x42::m {
fun main(_x: u64, _y: address) {}
}
