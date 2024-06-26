// options:
// printWidth: 70
// tabWidth: 4

module prettier::dot_expression {
    fun dot_expression() {
        board.place(1, 0); // black move
        board.assert_score(vector[0, 1, 0]); // empty / black / white

        board.first().second().ultra_long_third();
        board
            .start_a_chain()
            .then_call_something()
            .then_call_something_else();

        board
            .assert_score(vector[0, 1, 0])
            .assert_score()
            .assert_score(vector[0, 1, 0]);
        board.assert_state(vector[
            vector[2, 0, 0, 0],
            vector[1, 2, 0, 0],
            vector[0, 1, 0, 0],
            vector[1, 0, 0, 0],
        ]);

        board.place(2, 0); // white: Ko Rule!

    }
}
