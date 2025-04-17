// options:
// useModuleLabel: true

module match::test_module;

fun run(x: u64): u64 {
    match (x) {
        1 => 2,
        2 => 3,
        x => x,
    }
}

public struct Wrapper(u64)

// ERROR
fun add_under_wrapper_unless_equal(wrapper: Wrapper, x: u64): u64 {
    match (wrapper) {
        Wrapper(y) if (y == x) => Wrapper(y),
        Wrapper(y) => y + x,
    }
}

public enum MyEnum has drop {
    Variant(u64, bool),
    OtherVariant(bool, u64),
}

fun test_or_pattern(x: u64): u64 {
    match (x) {
        MyEnum::Variant(1 | 2 | 3, true) |
        MyEnum::OtherVariant(true, 1 | 2 | 3) => 1,
        MyEnum::Variant(8, true) | MyEnum::OtherVariant(_, 6 | 7) => 2,
        _ => 3,
    }
}

fun test_lit(x: u64): u8 {
    match (x) {
        1 => 2,
        2 => 3,
        _ => 4,
    }
}

fun test_var(x: u64): u64 {
    match (x) {
        y => y,
    }
}

const MyConstant: u64 = 10;

fun test_constant(x: u64): u64 {
    match (x) {
        MyConstant => 1,
        _ => 2,
    }
}

fun test_or_pattern(x: u64): u64 {
    match (x) {
        1 | 2 | 3 => 1,
        4 | 5 | 6 => 2,
        _ => 3,
    }
}

// ERROR
fun test_or_at_pattern(x: u64): u64 {
    match (x) {
        x @ 1 | 2 | 3 => x + 1,
        y @ 4 | 5 | 6 => y + 2,
        z => z + 3,
    }
}

fun f(x: MyEnum) {
    match (x) {
        MyEnum::Variant(1, true) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        MyEnum::Variant(..) => 3,
        MyEnum::OtherVariant(..) => 4,
    }
}

fun f(x: MyEnum) {
    match (x) {
        MyEnum::Variant(1 | 2 | 10, true) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        MyEnum::Variant(..) => 3,
        MyEnum::OtherVariant(..) => 4,
    }
}

public struct NonDrop(u64)

fun drop_nondrop(x: NonDrop) {
    match (x) {
        NonDrop(1) => 1,
        _ => 2,
        // ERROR: cannot wildcard match on a non-droppable value
    }
}

fun destructure_nondrop(x: NonDrop) {
    match (x) {
        NonDrop(1) => 1,
        NonDrop(_) => 2,
        // OK!
    }
}

fun use_nondrop(x: NonDrop): NonDrop {
    match (x) {
        NonDrop(1) => NonDrop(8),
        x => x,
    }
}

public enum MyEnum {
    Variant(u64, bool),
    OtherVariant(bool, u64),
}

public struct Pair<T>(T, T)

fun f(x: MyEnum): u8 {
    match (x) {
        MyEnum::Variant(1, true) => 1,
        MyEnum::Variant(_, _) => 1,
        MyEnum::OtherVariant(_, 3) => 2,
        // Now exhaustive since this will match all values of MyEnum::OtherVariant
        MyEnum::OtherVariant(..) => 2,
    }
}

fun match_pair_bool(x: Pair<bool>): u8 {
    match (x) {
        Pair(true, true) => 1,
        Pair(true, false) => 1,
        Pair(false, false) => 1,
        // Now exhaustive since this will match all values of Pair<bool>
        Pair(false, true) => 1,
    }
}

// ERROR
fun match_with_guard(x: u64): u64 {
    match (x) {
        1 if (false) => 1,
        1 => 2,
        _ => 3,
    }
}

public struct MyStruct(u64)

fun mut_on_immut(x: &MyStruct): u64 {
    match (x) {
        MyStruct(mut y) => {
            y = &(*y + 1);
            *y
        },
    }
}

fun mut_on_value(x: MyStruct): u64 {
    match (x) {
        MyStruct(mut y) => {
            *y = *y + 1;
            *y
        },
    }
}

fun mut_on_mut(x: &mut MyStruct): u64 {
    match (x) {
        MyStruct(mut y) => {
            *y = *y + 1;
            *y
        },
    }
}

public struct MyStruct(u64, u64, u64, u64) has drop;

public struct MyStruct2 {
    x: u64,
    y: u64,
    z: u64,
    w: u64,
}

fun wild_match(x: MyStruct) {
    match (x) {
        MyStruct(.., 1) => 1,
        // OK! The `..` pattern can be used at the beginning of the constructor pattern
        MyStruct(1, ..) => 2,
        // OK! The `..` pattern can be used at the end of the constructor pattern
        MyStruct(1, .., 1) => 3,
        // OK! The `..` pattern can be used at the middle of the constructor pattern
        MyStruct(1, .., 1, 1) => 4,
        MyStruct(..) => 5,
    }
}

fun wild_match2(x: MyStruct2) {
    match (x) {
        MyStruct2 { x: 1, .. } => 1,
        MyStruct2 { x: 1, w: 2, .. } => 2,
        MyStruct2 { .. } => 3,
    }
}

/// Convert a `u64` index to a `Colour`.
public fun from_index(index: u64): Colour {
    match (index) {
        0 => Colour::Empty,
        1 => Colour::Black, // ERROR
        2 => Colour::White, // ERROR
        _ => abort 0,
    }
}
