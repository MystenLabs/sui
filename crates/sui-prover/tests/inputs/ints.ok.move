module 0x42::foo;

#[spec_only]
use prover::prover::{ensures, requires};

public fun foo(x: u64): u64 {
  x + 1
}

#[spec(prove)]
public fun foo_spec(x: u64): u64 {
  requires(x < std::u64::max_value!());
  let res = foo(x);
  let x_int = x.to_int();
  let res_int = res.to_int();
  ensures(res_int == x_int.add(1u64.to_int()));
  res
}
