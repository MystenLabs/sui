//# run --args 42 42
// should fail for mismatched types
script {
fun main(_x: u64, _y: address) {}
}
