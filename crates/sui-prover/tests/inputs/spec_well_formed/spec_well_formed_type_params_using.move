module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo<T>() {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec<T>() {
  foo<u8>();
  ensures(true);
}
