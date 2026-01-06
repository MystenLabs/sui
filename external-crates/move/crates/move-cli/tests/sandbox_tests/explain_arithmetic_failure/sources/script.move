module 0x42::m {
    entry fun main() {
        1u64 - 2; // will cause integer underflow
    }
}
