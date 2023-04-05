//# run --args 0x1
// should fail, missing arg
script {
fun main(_x: u64, _y: address) {}
}
