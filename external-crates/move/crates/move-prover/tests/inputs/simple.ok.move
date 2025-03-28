module 0x42::foo;

public fun foo() {
  assert!(true);
}

#[spec(prove)]
fun foo_spec(x: u64) {
  foo();
}
