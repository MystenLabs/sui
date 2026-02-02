// Test: Complex type declarations including references and function types
// EBNF: Type, TypeArgs, TypeParameters, AbilityList
module 0x42::complex_types;

public struct Generic<T: copy + drop> has copy, drop { value: T }

public struct MultiParam<T, U: store, V: copy + drop + store> {
    first: T,
    second: U,
    third: V,
}

public struct WithPhantom<phantom T, U> { data: U }

public struct Nested<T: copy + drop> has copy, drop {
    inner: Generic<T>,
}

fun ref_types(x: &u64, y: &mut u64): &u64 {
    *y = *x + 1;
    x
}

fun tuple_type(): (u64, bool, u8) {
    (1, true, 255u8)
}

fun unit_type(): () {
    ()
}

macro fun with_fn_type<$T>($f: |u64| -> $T, $g: |$T, $T| -> bool): bool {
    let a = $f(1);
    let b = $f(2);
    $g(a, b)
}

macro fun zero_arg_fn<$T>($f: || -> $T): $T {
    $f()
}

fun type_cast(): u64 {
    let x: u8 = 10;
    let y: u16 = 1000;
    (x as u64) + (y as u64)
}

fun type_annotation(): u64 {
    let x: u64 = (10: u64);
    x
}
