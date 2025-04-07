module 0x42::foo;

#[spec_only]
use prover::prover::ensures;

public fun foo() {
  assert!(true);
}

public fun sub_foo(a: &mut u64) {
  assert!(true);
}


#[spec(prove)]
public fun foo_spec() {
  foo();

  let mut a = 5u64;

  sub_foo(&mut a);
  ensures(true);
}
