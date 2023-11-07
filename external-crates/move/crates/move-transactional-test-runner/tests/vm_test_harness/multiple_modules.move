//# publish

module 0x42::m {
    public fun foo() {
        0x42::n::bar()
    }
}

module 0x42::n {
    public fun bar() {}
}

//# run 0x42::m::foo

// mismatched addresses not supported
//# publish

module 0x44::m {
}
module 0x45::n {
}
