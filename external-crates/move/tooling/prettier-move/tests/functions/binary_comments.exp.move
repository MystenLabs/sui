// options:
// printWidth: 60
// useModuleLabel: true

module prettier::binary_comments;

fun chains(a: u64, b: u64, cond: bool, other: bool): bool {
    // a comment above the operator stays above the operator
    let x =
        a == b
            // choose the fallback
            || cond;

    // trailing comment mid-chain
    let y =
        cond // pick one
            && other
            && a > b;

    // trailing comment on an operand
    let z =
        a
            + b // sum
            + a * b;

    x && y && z > 0
}

fun block_rhs(p: bool, q: bool): bool {
    p && q && {
        let intermediate = p || q;
        intermediate == p
    }
}

fun precedence(a: u64, b: u64, c: u64, d: u64): bool {
    let long_mul =
        a * b * c * d * a * b * c * d * a * b * c * d;
    let mixed =
        a + b * c - d / a % b + a_very_long_name_here * c;
    let parens =
        (a + b) * (c - d) * (a + b) * (c - d) * (a + b);
    long_mul > 0 && mixed > 0 && parens > 0
}
