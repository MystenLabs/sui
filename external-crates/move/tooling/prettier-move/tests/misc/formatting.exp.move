// This is a line comment
/*

this is a multi-line block comment

*/

// This is a line comment

module tests::formatting {
    public struct Beep {
        transferred_to_object: VecMap<ID, /* owner */ ID>,
    }

    public fun list() {
        let a = vector[
            /* block */ 100, // hahaha
            /* block */ 200, // hihihi
        ];

        let b = vector[100, /* hahaha */  200 /* hihihi */];

        let c /* comment in between */ : vector<u64> = vector[
            100, // hahaha
            200, // hihihi
        ];

        let t: /* comment */ vector<u64> = vector<u64 /* hahaha */ >[100, 200];
    }

    public fun t_comment() {
        let very_long_variable =
            x"000000000000000000000000000000000000000000000000000000000000000000"; // t_comment

        if (
            x"000000000000000000000000000000000000000000000000000000000000000000" // t_comment
        ) {
            return;
        };
        function_call(
            x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
            x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
        );

        // leading commment
        dot.something().then().else(); // t_comment

        vector.length().do!(|el| el.something()); // t_comment
        vector.length().do!(|el| el.something().then().else()); // t_comment
        vector
            .length()
            .do!(
                |el| el
                    .something() /* I */
                    .then() /* Hate */
                    .else() /* You */
                    .and() /* Prettier */
                    .we()
                    .expect()
                    .breaking()
                    .right()
                    .here(),
            ); // t_comment

        dot
            .something() // t_comment
            .then() // t_comment
            .else(); // t_comment

        dot
            .dot()
            .function_call(
                // leading commment
                x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
                // leading commment
                x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
            );

        macro_call!(
            x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
            x"000000000000000000000000000000000000000000000000000000000000000000", // t_comment
        );
    }
}
