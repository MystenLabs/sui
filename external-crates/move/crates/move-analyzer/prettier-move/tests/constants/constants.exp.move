// options:
// printWidth: 60

/// Module level doc comment.
module test::constants {
    /// This is a constant.
    /// this comment should be attached to the node.
    /// what if there are 3 lines?
    const B: u8 =
        // line comment in between
        42;

    /// this is a line
    const A: u8 = 42;
    const T: u8 = 100; // trailing comment

    /// This is another comment;
    /// it should be attached to the node too.
    const C: u64 = 42;

    const T: vector<u8> =
        b"hello, cruel world. you have been there a long time.";

    const C: u64 = {
        100 +
        200 // trailing comment in a block
    };

    const X: u8 = 5 + 100 / 2 * 3;

    const V: vector<u8> = vector[1, 2, 3, 4, 5, 6];
    const V: vector<u8> = vector[
        1,
        2,
        3,
        4,
        5,
        6,
        7,
        8,
        9,
        10,
    ];
    const VV: vector<vector<u8>> = vector[
        vector[1, 2, 3],
        vector[4, 5, 6],
        vector[7, 8, 9],
    ];

    const CCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCCC: u64 =
        42;
    const D: TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT =
        42;
    const E: u64 =
        42000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000;
    const FFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFFF: TTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTTT =
        42000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000;
}
