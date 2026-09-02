// Unfoldable cross-module constants are reported at their uses in all forms: under
// short-circuiting operators (in constant definitions and function bodies) and in match patterns

module 0x42::a {

public(package) const BAD: bool = 1u64 / 0 == 0;

public(package) const BAD_N: u64 = 1 / 0;

}

module 0x42::b {

use 0x42::a;

const AND: bool = true && a::BAD;

const OR: bool = a::BAD || false;

public fun and_use(x: bool): bool {
    x && a::BAD
}

public fun match_use(x: u64): u64 {
    match (x) {
        a::BAD_N => 1,
        _ => 0,
    }
}

}
