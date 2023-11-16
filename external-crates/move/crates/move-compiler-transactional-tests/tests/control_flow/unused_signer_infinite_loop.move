//# run --gas-budget 700
module 0x42::m {
    fun main() {
        // out of gas
        loop ()
    }
}
