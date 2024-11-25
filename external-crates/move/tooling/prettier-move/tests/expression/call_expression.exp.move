// options:
// printWidth: 40
// tabWidth: 4
// useModuleLabel: true

module prettier::index_expression;

fun call_expression() {
    call(/* a */ 10, /* b */ 10);
    call(
        /* a */ 10,
        /* b */ 10,
        /* c */ 10,
    );
    call(/* a */ 10 /* trailing */);
}

fun misc() {
    some_really_cool_function_i_swear(
        /* read */ true,
        /* write */ true,
        /* transfer */ false,
        /* delete */ true,
    )
}
