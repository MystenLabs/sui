module 0x42::foo;

#[spec_only]
use prover::prover::requires;

public fun foo() {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec() {
  foo();
  requires(true);
}
