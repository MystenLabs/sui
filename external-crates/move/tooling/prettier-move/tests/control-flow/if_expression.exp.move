// options:
// printWidth: 80

module test::if_expression {
    public fun basic() {
        if (true) call_something();
        if (false)
            call_something_else_call_something_else_call_something_else();

        if (cond) do_this() else {
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

    public fun control_flow() {
        if (true) return call_something();
        if (true)
            return call_something_else_call_something_else_call_something_else();
    }

    public fun folding() {
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
}
