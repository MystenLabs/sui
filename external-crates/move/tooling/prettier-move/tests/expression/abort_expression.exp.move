// options:
// printWidth: 35
// useModuleLabel: true

module prettier::abort_expression;

// abort can be with a value or
// without a value, then the space
// is not printed
fun abort_expr() {
    none().destroy_or!(abort);
    abort 1337;
    abort (1);

    // tests comments
    abort /* hello */ 10;
    /* abort */ abort // 111
        10; // and trailing ones

    abort
}
