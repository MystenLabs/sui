module 0x42::M {

    fun func1(x: u64) {
        let _b = x << 24;
        let _b = x << 64; // Should raise an issue
        let _b = x << 65; // Should raise an issue
        let _b = x >> 66; // Should raise an issue
    }
}