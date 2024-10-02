module Completion::uses {
    use Completion::dot::aliased;
    use Completion::{dot::{foo, bar}, colon_colon::complete_chains};

    public fun foo(p: SomeStruct) {}

    public fun use_fun_chain(s: SomeStruct) {
        use Completion::colon_colon as CC;
        use fun CC::foo as Completion::colon_colon::SomeStruct.foo_alias;

        s.foo_alias();
    }

    public fun partial1() {
        use C
    }

    public fun partial2() {
        use Completion::
    }

    public fun partial3() {
        use Completion::dot::
    }

    public fun partial4() {
        use Completion::dot::f
    }

    public fun partial5() {
        use Completion::dot::{
    }

    public fun reset_parser1() {}

    public fun partial6() {
        use Completion::dot::{f
    }

    public fun reset_parser1() {}

    public fun partial7() {
        use Completion::{
    }

    public fun reset_parser2() {}

    public fun partial8() {
        use Completion::{d
    }

    public fun reset_parser3() {}

    public fun partial9() {
        use Completion::{dot::
    }

    public fun reset_parser4() {}

    public fun partial10() {
        use Completion::{dot::f
    }

    public fun reset_parser5() {}

    public fun partial11() {
        use Completion::{dot::{foo, b
    }

    public fun reset_parser6() {}

    public fun partial12() {
        use Completion::{dot::{foo, bar}, c
    }

    public fun reset_parser7() {}
}
