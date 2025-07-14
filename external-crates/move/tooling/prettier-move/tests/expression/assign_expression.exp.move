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
    assign = if (long_condition)
        long_expression
    else another_long_expression;

    assign = if (true) { a } else {
        b
    };

    // assignment with if + block
    assign = if (true) {
        long_true_expression
    } else {
        long_false_expression
    };

    // assignment + control flow
    assign = 'ret: if (true) {
        long_true_expression
    } else {
        long_false_expression
    };

    assign = 'loop: loop {
        break 'loop a;
    };

    assign = 'ret: 10u8.do!(|x| {
        return 'ret x;
    });

    // Block

    assign = {
        // comment
        a = b;
        a
    };

    assign = vector[100, 200];
    assign = vector[
        100000,
        200000,
        30000,
    ];

    // assign + function call
    deny_cap_v2 =
        some::thing(deny_cap);
}

fun assign_comment() {
    a /* before */ = /* after */ b;

    // leading
    a = // comment
        b /* b */; // trailing

    a = /* hahaha */ b;

    // two types of trailing comments
    // are mixed in lhs
    a /* unsolved */ = // comment
        b /* b */; // trailing

    a = // comment
        if (true) expr
        else longer_expression;
}
