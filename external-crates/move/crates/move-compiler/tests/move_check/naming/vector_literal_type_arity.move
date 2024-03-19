module 0x42::Test {
    fun t() {
        // check that vector expects 1 type argument
        let v0 = vector<>[]; v0;
        let v2 = vector<u64, bool>[0, false]; v2;
    }
}
