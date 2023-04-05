//# run --gas-budget 700 --signers 0x1
script {
    fun main(_s: signer) {
        // out of gas
        loop ()
    }
}
