// options:
// printWidth: 60
// tabWidth: 4

module test::while_expression {
    fun test_while() {
        // hey yall
        'a: {};

        // comments
        while (true) {
            break;
        };
        // hahaha
        'a: while (
            very_very_long_condition ||
            very_very_long_condition ||
            very_very_long_condition ||
            very_very_long_condition
        ) {
            break;
        };
    }
}
