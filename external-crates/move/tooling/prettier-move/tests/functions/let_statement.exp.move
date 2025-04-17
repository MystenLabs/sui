// options:
// printWidth: 60

// Testing strategy for let statement:
//
// - bind list / expression list matches, mutability
// - test breakable expressions like expression_list
// - test let assignment with control flow like if, while, loop
// - test let with very long unbreakable expressions
// - test let with long breakable expressions
// - write every example how it should be formatted
module prettier::let_statement {
    fun basic() {
        let _;
        let mut c;
        let a = 1;
        let x: u64;
        let (a, b) = (1, 2);
        let c: vector<u8> = vector[1, 2, 3];
        let (mut a, mut b) = (1, 2);
        let (
            mut something,
            something_else,
            mut another_thing,
        ) = (1, 2, 3);
    }

    fun breakable_expressions() {
        let something = vector[
            very_long_element_of_the_vector,
        ];

        let (item1, item2) = (
            // breaks here
            very_long_expression,
            very_long_expression,
        );

        let item = Struct {
            that: vector[],
            packs: 1000,
            itself: false,
        };

        let item = chain
            .that_does()
            .truly_magically()
            .breaks_itself();

        let item = {
            block;
            will_be;
            block
        };

        let item = macro_call!(
            very_long_list_expression_1,
            very_long_list_expression_2,
        );
    }

    fun unbreakable_expressions() {
        let something =
            very_long_expression_that_does_not_break_itself;
        let Kiosk { id, profits, owner: _, item_count } =
            self;
    }

    fun break_list() {
        let block = {
            let a = 1;
            let b = 2;
            let c = 3;
            a + b + c
        };

        let v = vector[
            vector[1, 2, 3],
            vector[4, 5, 6],
            vector[7, 8, 9],
        ];

        let (a, mut b, c) = (
            very_long_list_expression_1,
            very_long_list_expression_2,
            very_long_list_expression_3,
        );
    }

    fun break_long_value() {
        let (a, b, c) = (
            very_very_very_long_value,
            very_very_very_long_value,
            very_very_very_long_value,
        );

        let z = first().final().second_arg();

        let (
            very_long_binding,
            very_long_binding1,
            very_long_binding2,
        ) = (1, 2, 3);

        let (
            very_long_binding,
            mut very_long_binding1,
            very_long_binding2,
        ) = (
            very_very_very_long_value,
            very_very_very_long_value,
            very_very_very_long_value,
        );

        let a =
            very_very_very_long_value_very_long_value_very_long_value;

        let c: TypeName<Which<Is<Very<Big>>>> =
            very_very_very_long_value_very_long_value_very_long_value;

        let to_remain_locked = (
            self.final_unlock_ts_sec -
                math::min(self.final_unlock_ts_sec, now),
        );

        let to_remain_locked =
            (
            self.final_unlock_ts_sec - math::min(self.final_unlock_ts_sec, now)
        ) * self.unlock_per_second;

        let locked_amount_round =
            balance::value(&self.locked_balance) / self.unlock_per_second * self.unlock_per_second;
    }

    fun misc() {
        let a =
            very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition &&
        very_very_long_if_condition > very_very_long_if_condition;
    }

    fun let_match() {
        let mut stone = match (e.is_black()) {
            true => shape::use_(b"#b".to_string()),
            false => shape::use_(b"#w".to_string()),
        };
    }
}
