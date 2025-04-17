// options:
// printWidth: 35
// tabWidth: 4
// useModuleLabel: true

module prettier::dot_expression;

fun vector_simple() {
    // should be printed as is
    vector[];

    // should fit on one line
    vector[alice, bob, carl, dave];

    // should be expanded to multiple lines
    vector[
        alice,
        bob,
        carl,
        dave,
        eve,
    ];
}

// we have 3 list types: vector, block, and
// expression list
fun vector_lists() {
    // fits one line
    vector[vector[], vector[]];

    // expanded to multiple lines
    vector[
        vector[],
        vector[],
        vector[],
    ];

    // expanded, with each vector on a new line
    vector[
        vector[1, 2, 3, 4, 5],
        vector[1, 2, 3, 4, 5],
    ];

    // any vector breaking in a list breaks the list
    // but not its elements
    vector[
        vector[alice, bob, carl],
        vector[alice, bob, carl],
        vector[
            alice,
            bob,
            carl,
            dave,
            eve,
        ],
    ];

    // block in a vector behaves like any other list
    // if it can be single-lined, it will, or break
    vector[{ 1 }, { 2 }, { 3 }];
    vector[
        { alice + bob + carl },
        { 2 },
        { 3 },
    ];

    // when block broken, it gets expanded and
    // indented like any other list
    vector[
        {
            let x = 1;
            let y = 2;
            x + y
        },
    ];

    vector[
        (
            alice + bob + carl + dave + smith,
        ),
    ];

    // expression list in a vector should always break
    vector[
        (alice + bob + carl),
        (bob),
        (carl),
    ];

    // any single list in a vector
}

fun vector_formatting() {
    // leading comment
    vector[]; // trailing comment

    // line comment breaks the list
    // TODO:
    vector[
        alice, // alice
        // bob,
        carl, // that dude
        // trailing comment kept
        // another comment
        // and one more
    ];

    // leading comment
    vector[
        /* alice */ 1,
        /* bob */ 2,
        /* carl */ 3,
    ]; // trailing comment

    vector[
        // hey there
        alice,
        bob,
        carl,
    ];
}

fun vector_type_args() {
    // vector with type arguments, should not break
    vector<Type>[alice, bob];

    // elements break, but type arguments don't
    vector<Type>[
        alice,
        bob,
        carl,
        eve,
        dave,
    ];

    // should try and break on elements, until
    // it can't anymore
    vector<Collection<Al, Bo, Ca>>[
        alice,
    ];

    // should break on type arguments if breaking
    // elements is not enough to fit on one line
    vector<Collection<
        Alice,
        Bob,
        Carl,
    >>[alice];
}
