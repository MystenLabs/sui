// options:
// printWidth: 35
// useModuleLabel: true

module test::if_expression;

fun basic() {
    if (true) call_something();
    if (false) call_something_else_call_something_else_call_something_else();


    if (cond) do_this()
    else {
        do_that();
        do_this();
    };

    if (cond) {
        do_this();
        do_that();
    } else do_this();


    if (true) call_something()
    else call_something_else();

    if (false) {
        call_something_else();
    } else {
        another_call();
    };

    if (very_long_binding) {
        call_something();
    } else {
        call_something_else();
    };
}

fun control_flow() {
    if (true) return call_something();
    if (true) return call_something_else_call_something_else_call_something_else();
}

fun folding() {

    if (true) call_something()
    else if (false) call_something_else()
    else call_something_otherwise();

    if (true) {
        call_something()
    } else if (false) {
        call_something_else()
    } else {
        call_something_otherwise()
    };

    // should keep as is, no additions;
    if (true) { let a = b; };

    if (very_very_long_if_condition)
        very_very_long_if_condition > very_very_long_if_condition;

    let a = if (true) {
        call_something_else();
    } else {
        call_something_else();
    };

    if (
        very_very_long_if_condition > very_very_long_if_condition ||
        very_very_long_if_condition + very_very_long_if_condition > 100 &&
        very_very_long_if_condition
    )
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition;
    // should break list of expressions inside parens, with indent;
    if (
        very_very_long_if_condition > very_very_long_if_condition ||
        very_very_long_if_condition + very_very_long_if_condition > 100 &&
        very_very_long_if_condition
    ) {
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition;
    };

    if (very_very_long_if_condition > very_very_long_if_condition)
        return very_very > very_very_long_if_condition + 100;

    if (
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition
    )
        return very_very_long_if_condition > very_very_long_if_condition &&
            very_very_long_if_condition > very_very_long_if_condition ||
            very_very_long_if_condition > very_very_long_if_condition
}

fun if_expression() {
    // standard if-else
    if (true) true else false;

    // with block
    if (true) { true } else { false };

    // with multiline block
    if (true) {
        true
    } else {
        false
    };

    // mix block / no block
    if ({ expression }) {
       return expression
    } else expression;

    // reverse mix block / no block
    // should break on else (newline)
    if (expression) expression
    else {
        return expression
    };

    // should break on expression
    // and newline + indent it
    if (expression)
        expression_that_is_too_long
    else
        another_long_expression;

    // same exampla with block will
    // be broken into multiple lines
    if (expression) { expression_that_is_too_long }
    else { another_long_expression };

    if ({
        expression
    }) {
       return expression
    } else expression;
}

fun if_comments() {
    // comment associativity should be fixed
    // in the tree-sitter implementation by
    // adding names to `if`, `else` and `condition`
    if (/* expr */ expression) // comment
        expression
    else // comment
        expression;
}
