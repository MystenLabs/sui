//# run --signers 0x1
// should fail, missing signer
script {
fun main(_s1: signer, _s2: signer) {
}
}
