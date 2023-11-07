//# run --args 0x1 42
// should fail, flipped arguments
script {
fun main(_x: u64, _y: address) {}
}
