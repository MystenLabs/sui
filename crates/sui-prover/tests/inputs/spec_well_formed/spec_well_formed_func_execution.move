module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo(a: u8) {
  assert!(true);
}

#[spec(prove)]
public fun foo_spec(a: u8) {
  if (a > 3) {
    return;
  };

  foo(a);

  ensures(true);
}
