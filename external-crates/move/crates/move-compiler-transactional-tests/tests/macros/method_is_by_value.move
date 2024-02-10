//# init --edition 2024.alpha

//# publish

#[allow(dead_code, unused_assignment)]
module 42::m {
    public struct X() has copy, drop;

    public fun x_abort(): X { abort 0 }
    public fun id_abort(_: X): X { abort 1 }

    macro fun macro_abort($x: X) {
        abort 2;
        $x;
    }

    // method syntax results in the macro arg being bound first before being passed to the method
    // meaning these should abort from the LHS not the macro
    fun aborts0() {
        x_abort().macro_abort!();
    }

    fun aborts1() {
        X().id_abort().macro_abort!();
    }

    // The macro should abort here, since the arg is evaluated after the abort
    fun aborts2_not_0() {
        macro_abort!(x_abort());
    }

    fun aborts2_not_1() {
        macro_abort!(X().id_abort());
    }
}

//# run 42::m::aborts0

//# run 42::m::aborts1

//# run 42::m::aborts2_not_0

//# run 42::m::aborts2_not_1
