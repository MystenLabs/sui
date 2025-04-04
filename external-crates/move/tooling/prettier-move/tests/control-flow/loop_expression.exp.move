// options:
// printWidth: 35
// useModuleLabel: true

module test::loop_expression;

fun test_loop() {
    loop break;

    loop {
        break;
    };

    loop {
        break;
    };

    'a: loop {
        break 'a;
    };
}
