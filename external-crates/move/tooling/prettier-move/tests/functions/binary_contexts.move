// options:
// printWidth: 60
// useModuleLabel: true

module prettier::binary_contexts;

public struct Point { x: u64, y: u64 }

fun in_call_args(a: u64, b: u64): u64 {
    take_two(a + b * a - b, a * a + b * b - a * b + a + b + a);
    take_two(a + b, a - b)
}

fun in_assert(reinvest_rate: u64, slashing_rate: u64, denominator: u64) {
    assert!(reinvest_rate <= denominator && slashing_rate <= denominator, 0);
    assert!(
        reinvest_rate <= denominator && slashing_rate <= denominator && reinvest_rate > 0,
        1,
    );
}

fun in_vector(a: u64, b: u64): vector<u64> {
    vector[a + b, a - b, a * b + a * b + a * b + a * b + a * b + a * b]
}

fun in_pack(a: u64, b: u64): Point {
    Point { x: a + b * a - b + a, y: a * a + b * b - a * b + a + b }
}

fun in_index(v: &vector<u64>, i: u64): u64 {
    v[i + 1] + v[i * 2 - 1] + v[v.length() - i - 1 + i * 2 % 3]
}

public enum E { A(u64), B }

fun in_match_guard(e: &E, threshold: u64): u64 {
    match (e) {
        E::A(x) if (*x > threshold && *x % 2 == 0 && *x < threshold * 2) => 1,
        E::A(_) => 2,
        E::B => 3,
    }
}

fun in_lambda(v: vector<u64>, threshold: u64): u64 {
    let mut sum = 0;
    v.do!(|el| sum = sum + el * el + el % threshold + el / threshold - el);
    sum
}

fun in_loop_and_abort(a: u64, b: u64): u64 {
    let mut i = 0;
    while (i < a * b + a - b && i % 2 == 0 || i < a + b + a + b + a) {
        i = i + 1;
    };
    if (i > a * b + a * b + a * b + a * b + a * b + a * b) abort i + a * b;
    i
}

fun take_two(a: u64, b: u64): u64 {
    a + b
}
