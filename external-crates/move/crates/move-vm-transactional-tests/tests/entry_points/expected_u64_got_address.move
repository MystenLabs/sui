//# run --args @0x1
// should fail with mismatched types
module 0x42::m {
fun main(_x: u64) {}
}
