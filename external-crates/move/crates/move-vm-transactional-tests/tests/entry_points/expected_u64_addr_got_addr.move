//# run --args 0x1
// should fail, missing arg
module 0x42::m {
fun main(_x: u64, _y: address) {}
}
