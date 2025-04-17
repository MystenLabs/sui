// options:
// printWidth: 35
// tabWidth: 4
// useModuleLabel: true

module prettier::dot_expression;

fun dot_name() {
    // should not break, fits the line
    &board.elements.something.else;

    // should break, too long
    &board
        .elements
        .something
        .else
        .and_then;

    // in assignment we want to try and keep
    // the dot expression on the same line as
    // the `=` sign
    *some_identifier
        .get_mut(a, b)
        .next = next;

    // borrow expression without block does
    // not provide grouping, we need to make
    // sure it does not break
    &verified_issuer.issuer
}

fun dot_expression() {
    board.place(1, 0);
    board.score(vector[0, 1, 0]);

    board
        .first()
        .second()
        .ultra_long_third();
    board
        .start()
        .then()
        .then_else();

    board
        .score(vector[1, 0, 1])
        .score(
            vector[0, 1, 0],
            vector[0, 0, 1],
        )
        .assert_score()
        .assert_score(vector[
            0,
            1,
            0,
        ]);

    // a single dot expression should break if
    // the rhs is too long; this scenario is yet
    // to be implemented
    board.very_long_expression_will_it_indent_or_not();

    // with breakable lists, we should not break
    // the chain, unless the chain itself breaks
    board.add_element(Element {
        id: object::new(ctx),
    });

    // same as above, but with an arguments list
    board.add_fields(
        field_one,
        field_two,
        field_three,
    );

    // should not break, fits the line
    board.assert_state(vector[]);

    // TODO: come back to this example
    //
    // should not break, because vector inside
    // is a breakable expression and knows how
    // to break itself
    board.assert_state(vector[
        vector[2, 0, 0, 0],
        vector[1, 2, 0, 0],
        vector[0, 1, 0, 0],
        vector[1, 0, 0, 0],
    ]);

    // trailing and leading comments do not
    // break the chain
    board.place(2, 0); // trail
}

fun dot_comments() {
    // element fits the line but breaks because
    // of the comment in between the chain
    dot
        .some() // t
        .else(); // t

    // breaks all chain, comment follows
    dot.something().then().else(); // t_comment

    // fits the line, comment is attached
    expr.function_call(); // trail

    // breaks correctly, keeps comments where
    // they belong
    expression
        // lead
        .function_call(100) // trail
        // illustrates real-world scenario
        // when code is commented out
        // .function_call(); // trail
        .function_call(); // trail

    // leading and trailing comments should be kept
    // in place
    expression
        // lead
        .div(50); // trailing
}

fun dot_with_lambda_lists() {
    // should not indent unless breaks
    vector.length().do!(|el| {
        el.destroy_empty();
    });

    // should indent, breaks as expected
    vector
        .length()
        .do_range!(num, |el| {
            el.destroy_empty();
        });

    // should break and indent correctly
    option
        .map!(|e| option::some(10))
        .destroy_or!({
            let _ = 1;
            1 + 2
        });

    // should not break nor add extra indent
    object.option.destroy_or!({
        let _ = 1;
        1 + 2
    });

    // should not break nor add extra indent
    // even with line comments inside the block
    object.option.destroy_or!({
        // comment
        1 + 2
    });
}
