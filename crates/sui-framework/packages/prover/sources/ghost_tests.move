#[allow(unused)]
module prover::ghost_tests;

use std::u64;

#[spec_only]
use prover::prover::{requires, ensures, asserts};
#[spec_only]
use prover::ghost;

fun inc(x: u64): u64 {
  x + 1
}

public struct GhostStruct {}

#[spec]
fun inc_spec(x: u64): u64 {
  ghost::declare_global_mut<GhostStruct, bool>();
  requires(ghost::global<GhostStruct, _>() == false);

  asserts((x as u128) + 1 <= u64::max_value!() as u128);

  let result = inc(x);

  ensures(result == x + 1);
  ensures(ghost::global<GhostStruct, _>() == true);

  result
}

fun inc_saturated(x: u64): u64 {
  if (x == u64::max_value!()) {
    x
  } else {
    inc(x)
  }
}

#[spec(prove)]
fun inc_saturated_spec(x: u64): u64 {
  ghost::declare_global_mut<GhostStruct, bool>();
  requires(ghost::global<GhostStruct, _>() == false);

  let result = inc_saturated(x);

  ensures((ghost::global<GhostStruct, _>() == true) == (x != u64::max_value!()));

  result
}

public struct Wrapper<T> {
  value: T,
}

#[spec(prove)]
fun wrapper_well_formed_spec() {
  ghost::declare_global<GhostStruct, Wrapper<u64>>();
  ensures(ghost::global<GhostStruct, Wrapper<u64>>().value <= u64::max_value!());
}

#[spec_only]
fun set_test() {
  ghost::set<GhostStruct, bool>(&true);
}

#[spec(prove)]
fun set_test_spec() {
  ghost::declare_global_mut<GhostStruct, bool>();
  set_test();
  ensures(ghost::global<GhostStruct, bool>() == true);
}

#[spec(prove)]
fun ghost_borrow_mut_spec() {
  ghost::declare_global_mut<GhostStruct, bool>();
  let ghost_ref = ghost::borrow_mut<GhostStruct, bool>();
  *ghost_ref = true;
  ensures(ghost::global<GhostStruct, bool>() == true);
}
