// options:
// printWidth: 35
// tabWidth: 4
// useModuleLabel: true

module prettier::index_expression;

fun index() {
    // index expression supports
    // simple cases
    let x = some_vec[0];

    // including other expressions
    let y = some_vec[{ x + 1 }];

    // index expression can break
    // if the expression is too
    // long
    let k = grid[
        first_element,
        second_element,
    ];

    // index expression supports
    // multiple indices
    *&mut grid[x0, y0] = num;
}
