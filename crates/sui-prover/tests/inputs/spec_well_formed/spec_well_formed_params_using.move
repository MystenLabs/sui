module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo(x: u128) {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec(x: u128) {
  foo(7u128);
  ensures(true);
}
