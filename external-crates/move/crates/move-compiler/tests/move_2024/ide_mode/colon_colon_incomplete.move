module a::m {

    public enum SomeEnum {
        SomeVariant,
    }

    public fun foo() {}

    public fun pkg_complete() {
        A
    }

    public fun mod_complete() {
        a::
    }

    public fun member_complete() {
        a::m::
    }

    public fun variant_incomplete() {
        a::m::SomeEnum::
    }

}

module a::m2 {
    public fun foo() {}
}
