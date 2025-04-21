// options:
// printWidth: 40
// tabWidth: 4
// useModuleLabel: true

module prettier::call_expression;

fun call_expression() {

    call(/* a */ 10, /* b */ 10);
    call(/* a */ 10, /* b */ 10, /* c */ 10);
    call(/* a */ 10 /* trailing */);
    call(
        // leading
        10,
    );
    call(
        10, // trailing
    );
    call(
        /* a */ 10, // trailing line
    );

    call(if (cond) {
        b"1"
    } else {
        b"0"
    }.to_string());

}

fun misc() {
    some_really_cool_function_i_swear(/* read */ true, /* write */ true, /* transfer */ false, /* delete */ true)
}
