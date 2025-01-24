// option:
// print_width: 80

module test::return_expression {
    public fun folding() {
        // this is a return expression
        return very_very > very_very_long_if_condition + 100;
        return very_very_long_if_condition > very_very_long_if_condition &&
                very_very_long_if_condition > very_very_long_if_condition ||
                very_very_long_if_condition > very_very_long_if_condition; // this is a trailing comment for return
        return very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition;
        return {
            if (block_expression()) 1
            else 2
        };

        // and top comment
        return; // another trailing

        return { a + b };

        return vector[
            first_return_value,
            second_return_value,
            third_return_value,
        ];

        if (cond) {
            do_something_very_very_nasty_and_its_super_long_too_hahahahaa()
        } else return;

        return ({ a + b }, first_return_value, second_return_value);

        return if (some_value) {
            some_value
        } else {
            some_other_value
        };

        return if (some_value) some_value
        else some_other_value;

        return (
            very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition,
            very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition,
            very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition_very_very_long_if_condition,
        );
    }
}
