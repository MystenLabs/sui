// options:
// printWidth: 35
// useModuleLabel: true

module test::while_expression;

fun test_while() {
    // hey yall
    /* leading */ 'a: {}; // trailing

    // comments
    while (true) {
        break;
    }; // trailing
    // hahaha
    'a: /* a */  while (/* b */
        very_very_long_condition ||
        very_very_long_condition ||
        very_very_long_condition ||
        very_very_long_condition) // trailing
    {
        break;
    };
}
