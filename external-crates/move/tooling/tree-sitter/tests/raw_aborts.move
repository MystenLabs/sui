module a::aborts;

fun test_unable_to_destroy_non_zero() {
    abort;

    abort abort abort;

    abort
}
