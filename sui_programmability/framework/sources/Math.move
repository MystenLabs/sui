/// Basic math for nicer programmability
module Sui::Math {

    /// Return the larger of `x` and `y`
    public fun max(x: u64, y: u64): u64 {
        if (x > y) {
            x
        } else {
            y
        }
    }

    /// Return the smaller of `x` and `y`
    public fun min(x: u64, y: u64): u64 {
        if (x < y) {
            x
        } else {
            y
        }
    }
}
