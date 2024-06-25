// options:
// printWidth: 20

module prettier::abort_expression {
    fun abort_expression() {
        abort 0;
        abort {
            10
        };
        abort 100 + 300;
        abort if (condition) 100 else 200;
    }
}
