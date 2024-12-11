// options:
// printWidth: 40
// useModuleLabel: true

/// Covers `constant` node in grammar
module test::constants;

// standard constant, fits the line
const CONSTANT: vector<u8> = b"const"; // trailing

// breaks on long value, trying to break
// the value first
const CONSTANT: vector<u8> = vector[
    100, 200
];

// alternatively will newline the value
const CONSTANT: vector<u8> =
    b"constant_too_long";

// if there's a risk of breaking the type
// it will first try to break the value
const CONSTANT: vector<vector<u8>> =
    vector[];

// however, types will still break if
// there's no other option. A rare case.
const CONSTANT_ADDRESS: vector<
    vector<address>
> = vector[1, 2, 3];

// blocks are supported, both single and
// multi line
const CONSTANT: u64 = { 100 + 200 };
const CONSTANT: u64 = {
    100 + 200 + 500
};

// vectors of numbers will be "filled"
// instead of being expanded vertically
const CONSTANT: vector<u64> = vector[
    10000, 20000, 30000, 40000, 50000,
    60000, 70000, 80000, 90000, 100000,
];

// same applies to vectors of "bool"s
const CONSTANT: vector<bool> = vector[
    true, false, true, false, true,
];

// but does not apply to addresses
const CONSTANT: vector<address> =
    vector[
        @0xA11CE,
        @0xB0B,
        @0xCA41,
        @0xB004
    ];

// comments break the list always
const MOVES_POWER: vector<u8> = vector[
    40, // Rock
    60, // Paper
    80, // Scissors
];

// === Comments ===

const /* A */ CONSTANT: /* B */ u8 = /* C */ 1000; // trailing

const CONSTANT: u8 =
    // line comment
    1000;


// === Misc / Leftovers ===

/// this is a line
const A: u8 = 42;
const T: u8 = 100; // trailing comment

/// This is another comment;
/// it should be attached to the node too.
const CONSTANT: u64 = 42;

// bytestring literal
const CONSTANT: vector<u8> =
    b"hello cruel world!";

// hex literal
const CONSTANT: vector<u8> =
    x"AAAAAAAAAAAAAAAAAA";

// block
const CONSTANT: u64 = {
    100 + 200 // trailing comment in a block
};

// expression without a block
const CONSTANT: u8 = 5 + 100 / 2 * 3;

// expressions will be kept on a single
// line whenever possible and not break
const CONSTANT: u8 =
    100000 * 200000 * 30000;

// different vectors
const CONSTANT: vector<u8> = vector[
    1, 2, 3, 4, 5, 6
];
const CONSTANT: vector<u8> = vector[
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10
];
const CONSTANT: vector<vector<u8>> =
    vector[
        vector[1, 2, 3],
        vector[4, 5, 6],
        vector[7, 8, 9],
    ];
