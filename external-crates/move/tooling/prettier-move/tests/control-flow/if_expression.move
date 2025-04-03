// options:
// printWidth: 35
// useModuleLabel: true

module test::if_expression;

fun basic() {
    if (true) call_something();
    if (false) call_some_long_func();


    if (cond) do_this() else {
        do_that();
        do_this();
    };

    if (cond) push_else_out() else {
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
    if (true) break;
    if (true) abort;
    if (true) return call();
    if (true) return call_something();

    'a: if (true) { break 'a; }
}

fun dot_expression() {
    if (true) a.dot().expression()
    else b.dot().expression();

    if (true)
        staking
            .pools[pool]
            .borrow_mut()
            .unwrap()
            .stake(amount)
    else
        staking
            .pools[pool]
            .borrow_mut()
            .stake(0)
}

fun lists() {
    if (true) (1, 2, 3)
    else (4, 5, 6);

    // list can break itself
    if (true) (longer, list, values)
    else (short, list, third);

    // trailing comment on the true
    // branch forces else on a new
    // line, other comments work
    if (true) /* wow */ (longer, /* haha */ list, values) // beep
    else (short, list, third); // boop

    // both lists are broken, no
    // braces added to any branch
    if (true) (longer, list, values) // beep
    else (even, longer, list, better); // boop

    // vector expression is another
    // list that is supported
    if (true) vector[1, 2, 3, 4];

    // printed in 1 line
    if (true) vector[] else abort;

    // printed in 2 lines
    if (true) vector[1, 2, 3, 4]
    else abort;

    // does not break on else
    if (true) vector[1, 2, 3, 4]
    else vector[1, 2, 3, 4];

    // uses space if true breaks
    if (true) vector[100, 200, 300, 400]
    else vector[];

    // breaks on both braches
    if (true) vector[100, 200, 300, 400]
    else vector[100, 200, 300, 400, 500];

    // changes with trailing
    // comments
    if (true) vector[1, 2] // trailing
    else abort;

    // if list breaks, trailing
    // comment forces else newline
    if (true) vector[1, 2, 3, 4, 5] // trailing
    else abort;
}

fun folding() {
    if (true) call_something()
    else if (false) call_something_else()
    else last_call();

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
    // and add block to both
    // branches that break
    if (expression)
        expression_that_is_too_long
    else another_long_expression_too;

    // same example with block will
    // be broken into multiple lines
    if (expression) { expression_that_is_too_long }
    else { another_long_expression };

    if ({ expression }) {
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

    if (cond) expression
    // comment
    else expression; // comment
}

fun if_chaining() {
    if (true) 1
    else if (false) 2
    else if (true) 3
    else 4;
}

fun misc_tests() {
    if (condition) doesnt_break; // trailing
    if (condition) doesnt_break // trailing
    else doesnt_break; // another trailing

    if (48 <= b && b < 58) b - 48 // 0 .. 9
    else if (b == 1 || b == 80) 10 // p or P
    else if (b == 1 || b == 79) 11 // o or O
    else if (b == 1 || b == 84) 12 // t or T
    else if (b == 1 || b == 65) 13 // a or A
    else if (b == 1 || b == 69) 14 // e or E
    else if (b == 1 || b == 83) 15 // s or S
    else abort 1;

    let value = bcs.peel_u8();
    if (value == 0) false
    else if (value == 1) true
    else abort ENotBool
}
