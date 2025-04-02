module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo<T, K>() {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec<T, K>() {
  foo<K, T>();
  ensures(true);
}
