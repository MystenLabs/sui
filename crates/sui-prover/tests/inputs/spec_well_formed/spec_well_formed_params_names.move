module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo(x: u64): u64 {
  if (x == 0) {
    x
  } else {
    x - 1
  }
}

#[spec(prove)]
public fun foo_spec(a: u64): u64 {
  let result = foo(a);

  ensures(true);

  result
}
