//# init --edition 2024.alpha

//# publish
#[allow(all)]
module 42::m {
    macro fun ignore<$T>(_: $T) {}

    macro fun unused<$T>($x: $T) {}

    // unused macro arguments are not evaluated, so this does not abort
    fun does_not_abort() {
        ignore!(abort 0);
        unused!(abort 0);
    }
}

//# run 42::m::does_not_abort
