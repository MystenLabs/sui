// options:
// printWidth: 40

module prettier::expression {
    fun assign_expression() {
        a = 1;
        *a = 10;
        *&a = 100;
        a = (copy b);
        b = (move c);

        *df::borrow_mut<A, B>() =
            another_call();
        (a, b) = (1, 2);

        strategy.underlying_nominal_value_usdc =
            strategy.underlying_nominal_value_usdc - to_withdraw;
    }

    fun identified_expression() {
        'a: { call_something() };
    }

    fun name_expression() {
        name::expression;
        expression<T>;
    }

    fun call_expression() {
        call();
        call(1, 2, 3, 4, 5);
        call(Struct {}, Struct {});
        call(Struct {
            a: 1,
            b: 2,
            c: 3,
        });
        call(
            Struct { a: 1 },
            Struct { a: 1 },
        );
        call(
            nested_call(
                deeply_nested_call(),
            ),
            haha(),
        );
        call(
            nested_call(LongStruct {
                a: 1,
            }),
        );
    }

    fun call_expression_type_arguments() {
        call<InnerTypeArguments>();
        call<
            InnerTypeArguments,
            InnerTypeArguments,
        >();

        module::another_call<Type>(
            module::another_call<A>(),
            module::another_call<B>(),
            module::another_call<C>(),
        );

        module::another_call<
            TypeArgument,
        >(
            module::another_call<A>(),
            module::another_call<B>(),
            module::another_call<C>(),
        );
    }

    fun binary_expression() {
        lhs != rhs;
        lhs == rhs;
        lhs || rhs;
        lhs && rhs;
        lhs < rhs;
        lhs <= rhs;
        lhs > rhs;
        lhs >= rhs;
        lhs << rhs;
        lhs >> rhs;
        lhs + rhs;
        lhs - rhs;
        lhs * rhs;
        lhs / rhs;
        lhs % rhs;
        lhs & rhs;
        lhs | rhs;
        lhs ^ rhs;
    }

    fun binary_expression_folding() {
        // binary_expression
        say_something_really_long && say_something_really_long || say_something_really_long;

        say_something_really_long && say_something_really_long ||
        say_something_really_long;

        say_something_really_long && // trailing comment
        say_something_really_long ||
        // leading comment
        say_something_really_long;

        say_something_really_long + say_something_really_long + say_something_really_long &&
        say_something_really_long + say_something_really_long + say_something_really_long;

        say_something_really_long > less_than_or_equal_to &&
        say_something_really_long < greater_than_or_equal_to;

        (say_something_really_long + say_something_really_long + say_something_really_long) &&
        (say_something_really_long + say_something_really_long + say_something_really_long);

        (
            say_something + say_something + say_something + say_something + say_something,
        );
    }

    fun break_expression() {
        break;
        break 'a;

        'very_long_label: {
            break 'very_long_label;
        }
    }

    fun continue_expression() {
        continue;
        continue 'a;

        'very_long_label: {
            continue 'very_long_label;
        }
    }

    fun pack_expression() {
        Positional();
        Positional(
            1000,
            vector[10, 20, 30],
        );
        PackMe {};
        PackReallyShort {};
        PackWithF { field: 10 };
        PackMyStruct {
            id: object::new(ctx),
            name: b"hello".to_string(),
        };
        PackMyStruct<
            WithTypeParameters,
        > {
            id: object::new(ctx),
            name: b"hello".to_string(),
        };
        PackMyStruct {
            id: object::new(ctx),
            name: AnotherStruct {
                params: vector[
                    b"world".to_string(),
                ],
            },
        };
        PackMyStruct {
            id: object::new(ctx),
            point: P(10, 20),
            name: AnotherStruct {
                params: vector[
                    b"world".to_string(),
                ],
                haha: Another {
                    id: object::new(
                        ctx,
                    ),
                    param1: vector[
                        b"world".to_string(),
                        b"world".to_string(),
                        b"world".to_string(),
                        b"world".to_string(),
                    ],
                    param2: vector[
                        Packy {
                            x: 100000,
                            y: 500000,
                        },
                        Packy {
                            x: 100,
                            y: 200,
                        },
                    ],
                    param3: Positional(
                        b"very-very-very-long-argument-1",
                        b"very-very-very-long-argument-2",
                        b"very-very-very-long-argument-3",
                    ),
                },
            },
        };
    }

    fun vector_expression() {
        vector[10, 20, 30, 40];
        vector[
            b"say_something_familiar",
            b"to_me",
        ];
        vector[vector[10], vector[20]];
        vector[
            vector[10],
            vector[20],
            vector[30],
        ];
        vector<A>[S, T];
        vector<T<
            Extra,
            Long,
            Type,
            Arguments,
        >>[MyStructT];
    }

    fun unit_expression() {
        ();
    }

    fun index_expression() {
        say[0];
        double_index[variable][another][
            third_one,
        ];
        who_you_gonna_call[
            (first_arg, second_arg),
        ];
        result.push_back(copy data[i]);
    }

    fun expression_list() {
        (expr1, expr2, expr3);
        (expr1, expr2, expr3);
        (
            expr1,
            expr2,
            expr3,
            expr4,
            expr5,
        );
        (
            expr1,
            expr2,
            expr3,
            expr4,
            expr5,
            expr6,
            expr7,
            expr8,
            expr9,
            expr10,
        );
        (
            expr1,
            (expr2, expr3, expr4),
            expr5,
        );
        (
            expr1,
            (
                expr2,
                expr3,
                expr4,
                expr5,
            ),
            expr6,
            expr7,
            expr8,
            expr9,
            expr10,
        );

        let request = pool.finish_swap(
            request,
        );

        (
            request,
            swap_impl<
                CoinIn,
                CoinOut,
                LpCoin,
            >(
                pool,
                clock,
                coin_in,
                min_amount,
                ctx,
            ),
        )
    }

    fun annotate_expression() {
        (expr: Type);
        (
            really_long_expression: ReallyLongType
        ).say_something();
        let a =
            (call_expression(): Type);
        let a: Type = call_expression();
    }

    fun cast_expression() {
        100 as u64;
        1000 as u8;
        (say_something as u256);
    }

    fun dot_expression() {
        a().b().c();
        first()
            .second()
            .ultra_long_third();
        start_a_chain()
            .then_call_something()
            .then_call_something_else();

        let a = first()
            .second()
            .ultra_long_third();
        let a = first()
            .second()
            .ultra_long_third();
    }

    fun block() {
        {};
        { say_something(); };
        {
            should_this_be_so_long_or_not();
        };
        {
            say_something();
            say_something_else();
        };
        (
            first().second(),
            first()
                .second()
                .third()
                .fourth(),
        );
        {
            {
                say_something();
            };
            say_something_else();
            say_something_more();
            {};
        };
    }

    fun macro_module_access() {
        assert!<WithType, MultiEven>(
            module::call_something(),
        );
        assert!(
            module::call_something(),
            EDoesntQuiteWork,
        );
        say_hello::to_the_world!();
    }

    fun match_expression() {
        match (x) {
            NewEnum::V() => 0,
            NewEnum::V1(a, b) => {},
            NewEnum::V2 { x, y } => x,
            NewEnum::V3 => 0,
        }
    }
}
