module 0x42::M {

    fun func1(x: u64) {
        let _b = x << 24;
        let _b = x << 64; // <Issue:5>
        let _b = x << 65; // <Issue:5>
        let _b = x >> 66; // <Issue:5>
    }
}