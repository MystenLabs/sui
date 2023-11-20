//# run --gas-budget 700 --signers 0x1
module 0x42::m {
    fun main(_s: signer) {
        // out of gas
        loop ()
    }
}
