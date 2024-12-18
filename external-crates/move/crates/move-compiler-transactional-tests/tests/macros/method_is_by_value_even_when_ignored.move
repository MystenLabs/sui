//# init --edition 2024.alpha

//# publish

#[allow(dead_code, unused_assignment)]
module 42::m {
    public struct X() has copy, drop;

    public fun x_abort(): X { abort 0 }
    public fun id_abort(_: X): X { abort 1 }

    macro fun macro_abort(_: X) {
        abort 2
    }

    // method syntax results in the macro arg being bound first before being passed to the method
    // meaning these should abort from the LHS not the macro. Even though the LHS is discarded
    #[allow(dead_code)]
    fun aborts0() {
        x_abort().macro_abort!()
    }

    #[allow(dead_code)]
    fun aborts1() {
        X().id_abort().macro_abort!()
    }
}

//# run 42::m::aborts0

//# run 42::m::aborts1
