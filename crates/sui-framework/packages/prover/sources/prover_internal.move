module prover::prover_internal {
    native public fun begin_requires();
    native public fun end_requires();

    native public fun begin_ensures();
    native public fun end_ensures();

    native public fun begin_aborts();
    native public fun end_aborts();
    native public fun abort_if(p: bool);
}