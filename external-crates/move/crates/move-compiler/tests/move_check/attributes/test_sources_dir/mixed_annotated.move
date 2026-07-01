// First module is annotated; second is not. The warning should fire on the second module only.

#[test_only]
module a::annotated_first {
    fun foo() {}
}

module a::not_annotated_second {
    fun foo() {}
}
