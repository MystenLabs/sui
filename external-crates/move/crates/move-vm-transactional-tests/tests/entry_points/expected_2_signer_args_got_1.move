//# run --signers 0x1
// should fail, missing signer
module 0x42::m {
fun main(_s1: signer, _s2: signer) {
}
}
