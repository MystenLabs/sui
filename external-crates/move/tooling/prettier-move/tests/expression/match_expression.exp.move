// options:
// printWidth: 50
// tabWidth: 4
// useModuleLabel: true

// TODO: circle back on this once `match` has better support in tree-sitter.
// TODO: I suggest we do not format match expressions for now.
module prettier::pattern_matching;

fun f() {
    // empty match
    match (x) {};

    // empty match with a comment inside
    match (x) {
        // comment
    };

    // match with a very long expression
    // no comma (illegal)
    match (
        very_long_expression_that_needs_to_be_wrapped
    ) {
        Enum => 1,
    };

    // match with a breakable expression
    match ({
        let x = 0;
        x + 100
    }) {
        100 => 100,
        _ => 0,
    };

    // match with a single arm
    match (x) {
        Enum => 1,
    };

    // match with multiple arms
    match (x) {
        Enum => 1,
        Enum2 => 2,
    };

    // match with multiple arms and a
    // default arm
    match (x) {
        Enum => 1,
        Enum2 => 2,
        _ => 3,
    };

    // match with arm guards and complex
    // patterns
    match (x) {
        1 | 2 | 3 => 1,
        Enum if (x == 1) => 1,
        1 if (false) => 1,
        Wrapper(y) if (y == x) => Wrapper(y),
        x @ 1 | 2 | 3 => x + 1,
        z => z + 3,
        _ => 3,
    };
}

fun match_pair_bool(x: Pair<bool>): u8 {
    match (x) {
        Pair(true, true) => 1,
        Pair(true, false) => 1,
        Pair(false, false) => 1,
        Pair(false, true) => 1,
    }
}

fun incr(x: &mut u64) {
    *x = *x + 1;
}

fun match_with_guard_incr(x: u64): u64 {
    match (x) {
        x if ({ incr(&mut x); x == 1 }) => 1,
        // ERROR:    ^^^ invalid borrow of immutable value
        _ => 2,
    }
}

fun match_with_guard_incr2(x: &mut u64): u64 {
    match (x) {
        x if ({ incr(&mut x); x == 1 }) => 1,
        // ERROR:    ^^^ invalid borrow of immutable value
        _ => 2,
    }
}

//
fun use_enum() {
    let _local = tests::enum::test::A(10, 1000);
}

fun match_arms() {
    match (self.beep) {
        A::B(c) => {
            let c =
                c + (elements.length() as u16);
            if (is_hey(c, self.hey)) {
                self.beep =
                    A::C(clock.timestamp_ms());
            } else {
                self.beep = A::D(c);
            }
        },
        _ => {},
    };

    params.do!(|param| {
        match (self.beep) {
            A::B(c) => {
                let c =
                    c + (elements.length() as u16);
                if (is_hey(c, self.hey)) {
                    self.beep =
                        A::C(clock.timestamp_ms());
                } else {
                    self.beep = A::D(c);
                }
            },
            _ => {},
        }
    })
}
