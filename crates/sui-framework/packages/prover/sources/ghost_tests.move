module prover::ghost_tests;

use prover::prover::{requires, ensures, asserts, max_u64};
use prover::ghost::Self;

fun inc(x: u64): u64 {
    x + 1
}

public struct GhostStruct {}

#[ext(no_verify)]
fun inc_spec(x: u64): u64 {
    ghost::declare_global_mut<GhostStruct, bool>();
    requires(ghost::global<GhostStruct, _>() == false);

    asserts((x as u128) + 1 <= max_u64() as u128);

    let result = inc(x);

    ensures(result == x + 1);
    ensures(ghost::global<GhostStruct, _>() == true);

    result
}

fun inc_saturated(x: u64): u64 {
    if (x == max_u64()) {
        x
    } else {
        inc(x)
    }
}

fun inc_saturated_spec(x: u64): u64 {
    ghost::declare_global_mut<GhostStruct, bool>();
    requires(ghost::global<GhostStruct, _>() == false);

    let result = inc_saturated(x);

    ensures((ghost::global<GhostStruct, _>() == true) == (x != max_u64()));

    result
}

public struct Wrapper<T> {
    value: T
}

fun wrapper_well_formed_spec() {
    ghost::declare_global<GhostStruct, Wrapper<u64>>();
    ensures(ghost::global<GhostStruct, Wrapper<u64>>().value <= max_u64());
}
