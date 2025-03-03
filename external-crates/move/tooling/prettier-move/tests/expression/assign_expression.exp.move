// options:
// printWidth: 35
// tabWidth: 4
// useModuleLabel: true

module prettier::assign_expression;

fun assign_expression() {
    // straightforward example
    a = b;

    // assignment with if_expression
    a = if (true) { b } else { c };

    // assignment with if + break
    a =
        if (long_condition)
            long_expression
        else
            another_long_expression;

    a = if (true) { a } else { b };

    // assignment with if + block
    a = if (true) {
            long_true_expression
        } else {
            long_false_expression
        };
}

fun assign_comment() {
    a /* before */ = /* after */ b;

    // leading
    a = // comment
        b /* b */; // trailing

    a = /* hahaha */ b;

    a /* unsolved */ = // comment
        b /* b */; // trailing

    a = // comment
        if (true) expr
        else longer_expression;
}
