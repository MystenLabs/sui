module Macros::fun_type {

    entry fun entry_fun() {
    }

    macro fun macro_fun() {
    }

    public fun foo() {
        entry_fun();
        macro_fun!();
    }

}
