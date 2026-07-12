// options:
// printWidth: 60
// useModuleLabel: true

module prettier::binary_operators;

fun logical(a: bool, b: bool, c: bool): bool {
    a && b || c && a || b && c && a || b || c && a && b || c
}

fun comparison(a: u64, b: u64): bool {
    a == b || a != b || a < b || a > b || a <= b || a >= b
}

fun bitwise(a: u64, b: u64, c: u64): u64 {
    a & b | c ^ a & b | c ^ a & b | c ^ a & b | c ^ a & b | c
}

fun shifts(a: u64, b: u8): u64 {
    a << b >> b << b >> b << b >> b << b >> b << b >> b << b
}

fun arithmetic(a: u64, b: u64, c: u64): u64 {
    a + b - c + a % b * c / a + b - c * a / b % c + a - b + c
}

fun mixed_precedence(a: u64, b: u64, c: u64): bool {
    a + b << 2 > c & a || b * c <= a >> 1 && c % 2 == 0
}

fun unary_operands(a: bool, b: bool, v: &u64): bool {
    !a && !b || !(a || b) && *v > 0 && !(*v == 1)
}

fun reference_operands(a: &u64, b: &u64): bool {
    a == b && *a == *b && *a + *b > 0
}

fun cast_operands(a: u8, b: u8): u64 {
    (a as u64) + (b as u64) * (a as u64) - (b as u64) % (a as u64)
}

fun width_boundary(aaaaaa: u64, bbbbbb: u64): bool {
    // fits on one line at printWidth 60
    let fits = aaaaaa + bbbbbb + aaaaaa + bbbbbb > aaaaaa;
    // one operand longer: must break
    let breaks = aaaaaa + bbbbbb + aaaaaa + bbbbbb > aaaaaa + bbbbbb;
    fits && breaks
}
