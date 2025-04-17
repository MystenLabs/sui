// tests lints against combinable comparisons that can be simplified to a single comparison.
// these cases should work but do not
module a::m {
    fun negated(x: u64, y: u64) {
        !(x == y) || x > y;
    }
}
